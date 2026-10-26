//! Phase 2 compositor: stacks decoded frames on the GPU with wgpu, running
//! clip effects and transitions (WGSL, see [`crate::fx`]).
//!
//! Per frame:
//! 1. Each picture (a clip's decoded frame, RGBA8, straight alpha, sRGB) is
//!    uploaded as `Rgba8UnormSrgb` and converted ("prep") into a
//!    `Rgba16Float` texture: linear light, premultiplied alpha.
//! 2. Its effects run in order at the picture's own size, ping-ponging
//!    between two float textures.
//! 3. A transition places both pictures on canvas-size float textures and
//!    runs the transition shader into a third.
//! 4. Everything is drawn bottom to top with premultiplied "over" onto
//!    opaque black in an `Rgba8UnormSrgb` target (blending in linear light),
//!    then read back.
//!
//! Each picture is drawn as a quad: its rectangle, cropped, rotated around
//! its center and multiplied by its opacity ([`Xf`]); the default draws the
//! rectangle exactly as before transforms existed.
//!
//! Not here yet: anti-aliased edges on rotated pictures, Rec.709 vs sRGB
//! transfer differences, texture reuse between frames.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use wgpu::util::DeviceExt as _;

use crate::RenderError;
use crate::fx::{FIRST_IMAGE_BINDING, Fx, FxError, FxImage, Kind};

type Result<T> = std::result::Result<T, RenderError>;

/// One picture to place on the output (the old, effect-free entry point).
pub struct Layer<'a> {
    /// RGBA8, straight alpha, sRGB-encoded, `width * height * 4` bytes.
    pub rgba: &'a [u8],
    pub width: u32,
    pub height: u32,
    /// Where it lands on the output, in output pixels (may reach outside it).
    pub rect: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// How a picture is drawn into its rectangle: the part cut away on each
/// side, rotation around the rectangle's center, and opacity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Xf {
    /// Fractions of the picture cut away: left, top, right, bottom.
    pub crop: [f32; 4],
    /// Radians, clockwise on screen.
    pub rotation: f32,
    /// 0..1; multiplies the premultiplied colour.
    pub opacity: f32,
}

impl Default for Xf {
    fn default() -> Self {
        Self { crop: [0.0; 4], rotation: 0.0, opacity: 1.0 }
    }
}

/// One effect or transition pass with its inputs evaluated for this frame.
#[derive(Clone)]
pub struct Pass {
    pub fx: Arc<Fx>,
    /// Packed params ([`Fx::pack`]).
    pub params: Vec<u8>,
    /// Seconds since the clip (or transition) starts.
    pub time: f32,
    /// 0..1 across the clip (or transition).
    pub progress: f32,
    /// Pictures for the image params, in binding order ([`Fx::images_for`]).
    pub images: Vec<Arc<FxImage>>,
}

/// A decoded picture with its placement and effect stack.
pub struct Picture<'a> {
    pub rgba: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub rect: Rect,
    pub xf: Xf,
    pub effects: Vec<Pass>,
}

/// What one track contributes to a frame.
pub enum Item<'a> {
    Picture(Picture<'a>),
    Transition { outgoing: Picture<'a>, incoming: Picture<'a>, pass: Pass },
}

/// Draw a picture as a quad: corners in NDC (`a`: top left, top right;
/// `b`: bottom left, bottom right), the texture part `uv` (u0, v0, u1, v1),
/// opacity in `o.x`.
const RECT_SHADER: &str = r"
struct Place { a: vec4<f32>, b: vec4<f32>, uv: vec4<f32>, o: vec4<f32> };
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> place: Place;
struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
    );
    let c = corners[i];
    let top = mix(place.a.xy, place.a.zw, c.x);
    let bottom = mix(place.b.xy, place.b.zw, c.x);
    var out: VsOut;
    out.pos = vec4(mix(top, bottom, c.y), 0.0, 1.0);
    out.uv = mix(place.uv.xy, place.uv.zw, c);
    return out;
}
@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    return textureSampleLevel(tex, samp, in.uv, 0.0) * place.o.x;
}
";

/// Decoded sRGB texture (sampled as linear, straight alpha) to premultiplied float.
const PREP_SHADER: &str = r"
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    let xy = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: VsOut;
    o.pos = vec4<f32>(xy * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(xy.x, 1.0 - xy.y);
    return o;
}
@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(tex, samp, in.uv, 0.0);
    return vec4(c.rgb * c.a, c.a);
}
";

const OUT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const WORK: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Longest a test render may take before an effect counts as hanging.
const TEST_TIMEOUT: Duration = Duration::from_secs(2);

struct FxPipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

/// A GPU device with the compositing pipelines built.
pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    sampler: wgpu::Sampler,
    prep: wgpu::RenderPipeline,
    prep_layout: wgpu::BindGroupLayout,
    rect_layout: wgpu::BindGroupLayout,
    /// Rectangle draw into a float work texture (no blending).
    rect_work: wgpu::RenderPipeline,
    /// Rectangle draw onto the output (premultiplied over).
    rect_out: wgpu::RenderPipeline,
    fx: Mutex<HashMap<u64, Arc<FxPipeline>>>,
    /// Effect images on the GPU, by content hash. Few and small; kept for the process.
    images: Mutex<HashMap<u64, Arc<wgpu::Texture>>>,
    /// Adapter name and backend, for logs and diagnostics.
    pub adapter: String,
}

/// Run a future that wgpu resolves without an executor on native backends.
fn block_on<F: std::future::Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let waker = std::task::Waker::noop();
    let mut cx = std::task::Context::from_waker(waker);
    loop {
        if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::yield_now();
    }
}

fn gpu_err(e: impl std::fmt::Display) -> RenderError {
    RenderError::Io(format!("GPU: {e}"))
}

fn tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}

fn f16_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = (bits >> 10) & 0x1f;
    let frac = f32::from(bits & 0x3ff);
    match exp {
        0 => sign * frac * 2f32.powi(-24),
        31 => {
            if frac == 0.0 {
                sign * f32::INFINITY
            } else {
                f32::NAN
            }
        }
        e => sign * (1.0 + frac / 1024.0) * 2f32.powi(i32::from(e) - 15),
    }
}

/// The quad uniform for a picture in pixel rect `r`, drawn with `xf`, on a
/// `w x h` target (NDC, y up). With the default `Xf` the corners are exactly
/// the rect's.
fn quad(r: Rect, xf: Xf, w: u32, h: u32) -> Vec<u8> {
    let (w, h) = (w as f32, h as f32);
    let [l, t, rr, b] = xf.crop;
    let (x0, x1) = (r.x + l * r.w, r.x + r.w - rr * r.w);
    let (y0, y1) = (r.y + t * r.h, r.y + r.h - b * r.h);
    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    let (sin, cos) = xf.rotation.sin_cos();
    let turn = |x: f32, y: f32| -> (f32, f32) {
        if xf.rotation == 0.0 {
            return (x, y);
        }
        let (dx, dy) = (x - cx, y - cy);
        (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
    };
    let ndc = |(x, y): (f32, f32)| [x / w * 2.0 - 1.0, 1.0 - y / h * 2.0];
    let [tl, tr, bl, br] = [turn(x0, y0), turn(x1, y0), turn(x0, y1), turn(x1, y1)].map(ndc);
    [tl[0], tl[1], tr[0], tr[1], bl[0], bl[1], br[0], br[1], l, t, 1.0 - rr, 1.0 - b, xf.opacity, 0.0, 0.0, 0.0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect()
}

impl Gpu {
    /// The process-wide GPU, created on first use. Prefers a discrete GPU and
    /// falls back to any adapter, including a software one (WARP, llvmpipe).
    pub fn shared() -> Result<&'static Gpu> {
        static GPU: OnceLock<std::result::Result<Gpu, String>> = OnceLock::new();
        GPU.get_or_init(|| Gpu::new().map_err(|e| e.to_string())).as_ref().map_err(|e| RenderError::Io(e.clone()))
    }

    pub fn new() -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let options = |fallback| wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: fallback,
            compatible_surface: None,
            ..Default::default()
        };
        let adapter = block_on(instance.request_adapter(&options(false)))
            .or_else(|_| block_on(instance.request_adapter(&options(true))))
            .map_err(|e| gpu_err(format!("no usable adapter: {e}")))?;
        let info = adapter.get_info();
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("ve compositor"),
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .map_err(gpu_err)?;

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ve"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let prep_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("prep"),
            entries: &[tex_entry(0), sampler_entry(1)],
        });
        let rect_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rect"),
            entries: &[tex_entry(0), sampler_entry(1), uniform_entry(2, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let make = |label: &str, src: &str, layout: &wgpu::BindGroupLayout, format, blend| {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(label), source: wgpu::ShaderSource::Wgsl(src.into()) });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pl),
                vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let prep = make("prep", PREP_SHADER, &prep_layout, WORK, None);
        let rect_work = make("rect work", RECT_SHADER, &rect_layout, WORK, None);
        let rect_out = make("rect out", RECT_SHADER, &rect_layout, OUT, Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING));
        Ok(Self {
            device,
            queue,
            sampler,
            prep,
            prep_layout,
            rect_layout,
            rect_work,
            rect_out,
            fx: Mutex::new(HashMap::new()),
            images: Mutex::new(HashMap::new()),
            adapter: format!("{} ({:?})", info.name, info.backend),
        })
    }

    fn work_texture(&self, w: u32, h: u32) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("work"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: WORK,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }

    fn uniform(&self, bytes: &[u8]) -> wgpu::Buffer {
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("uniform"), contents: bytes, usage: wgpu::BufferUsages::UNIFORM })
    }

    /// The pipeline for an effect or transition, compiled once per code hash.
    fn fx_pipeline(&self, fx: &Fx) -> std::result::Result<Arc<FxPipeline>, FxError> {
        if let Some(p) = self.fx.lock().ok().and_then(|m| m.get(&fx.hash).cloned()) {
            return Ok(p);
        }
        let mut entries = vec![tex_entry(0), sampler_entry(1), uniform_entry(2, wgpu::ShaderStages::FRAGMENT), uniform_entry(3, wgpu::ShaderStages::FRAGMENT)];
        if fx.kind() == Kind::Transition {
            entries.push(tex_entry(4));
        }
        for i in 0..fx.image_params().count() {
            entries.push(tex_entry(FIRST_IMAGE_BINDING + i as u32));
        }
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let (source, _) = fx.wrapped();
        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&fx.manifest.id),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let layout = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some(&fx.manifest.id), entries: &entries });
        let pl = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&fx.manifest.id),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&fx.manifest.id),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("ve_vs"), compilation_options: Default::default(), buffers: &[] },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("ve_fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: WORK, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        if let Some(e) = block_on(scope.pop()) {
            return Err(FxError(format!("{}: the GPU rejected the shader: {e}", fx.manifest.shader)));
        }
        let p = Arc::new(FxPipeline { pipeline, layout });
        if let Ok(mut m) = self.fx.lock() {
            m.insert(fx.hash, Arc::clone(&p));
        }
        Ok(p)
    }

    /// Encode one effect or transition pass from `a` (and `b`) into `out`.
    fn encode_pass(
        &self,
        enc: &mut wgpu::CommandEncoder,
        pass: &Pass,
        pipe: &FxPipeline,
        (a, b): (&wgpu::TextureView, Option<&wgpu::TextureView>),
        out: &wgpu::TextureView,
        size: (u32, u32),
    ) {
        let mut t = Vec::with_capacity(16);
        for v in [pass.time, pass.progress, size.0 as f32, size.1 as f32] {
            t.extend_from_slice(&v.to_le_bytes());
        }
        let ut = self.uniform(&t);
        let up = self.uniform(&pass.params);
        let mut entries = vec![
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(a) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            wgpu::BindGroupEntry { binding: 2, resource: ut.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: up.as_entire_binding() },
        ];
        if let Some(b) = b {
            entries.push(wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(b) });
        }
        let views: Vec<wgpu::TextureView> = pass.images.iter().map(|img| self.image(img).create_view(&Default::default())).collect();
        for (i, v) in views.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry { binding: FIRST_IMAGE_BINDING + i as u32, resource: wgpu::BindingResource::TextureView(v) });
        }
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("fx"), layout: &pipe.layout, entries: &entries });
        let mut rp = begin(enc, out, wgpu::Color::TRANSPARENT);
        rp.set_pipeline(&pipe.pipeline);
        rp.set_bind_group(0, &group, &[]);
        rp.draw(0..3, 0..1);
    }

    /// An effect image as a texture, uploaded once. `Rgba8Unorm`, not sRGB:
    /// the shader reads the stored values (a luma map is data, not light).
    fn image(&self, img: &FxImage) -> Arc<wgpu::Texture> {
        if let Some(t) = self.images.lock().ok().and_then(|m| m.get(&img.hash).cloned()) {
            return t;
        }
        let size = wgpu::Extent3d { width: img.width, height: img.height, depth_or_array_layers: 1 };
        let t = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&img.name),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &t, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &img.rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(img.width * 4), rows_per_image: Some(img.height) },
            size,
        );
        let t = Arc::new(t);
        if let Ok(mut m) = self.images.lock() {
            m.insert(img.hash, Arc::clone(&t));
        }
        t
    }

    /// Upload, prep and run the effect stack of a picture. Returns the
    /// resulting float texture (picture size).
    fn picture(&self, enc: &mut wgpu::CommandEncoder, pic: &Picture<'_>) -> Result<wgpu::Texture> {
        if pic.rgba.len() != (pic.width as usize) * (pic.height as usize) * 4 || pic.width == 0 || pic.height == 0 {
            return Err(RenderError::Invalid(format!("picture is {}x{} but has {} bytes", pic.width, pic.height, pic.rgba.len())));
        }
        let size = wgpu::Extent3d { width: pic.width, height: pic.height, depth_or_array_layers: 1 };
        let upload = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("upload"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: OUT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &upload, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            pic.rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(pic.width * 4), rows_per_image: Some(pic.height) },
            size,
        );
        let mut cur = self.work_texture(pic.width, pic.height);
        {
            let view = upload.create_view(&Default::default());
            let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("prep"),
                layout: &self.prep_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            });
            let out = cur.create_view(&Default::default());
            let mut rp = begin(enc, &out, wgpu::Color::TRANSPARENT);
            rp.set_pipeline(&self.prep);
            rp.set_bind_group(0, &group, &[]);
            rp.draw(0..3, 0..1);
        }
        for pass in &pic.effects {
            let pipe = self.fx_pipeline(&pass.fx).map_err(|e| RenderError::Invalid(e.0))?;
            let next = self.work_texture(pic.width, pic.height);
            let (a, o) = (cur.create_view(&Default::default()), next.create_view(&Default::default()));
            self.encode_pass(enc, pass, &pipe, (&a, None), &o, (pic.width, pic.height));
            cur = next;
        }
        Ok(cur)
    }

    fn rect_group(&self, view: &wgpu::TextureView, rect: Rect, xf: Xf, w: u32, h: u32) -> wgpu::BindGroup {
        let place = self.uniform(&quad(rect, xf, w, h));
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rect"),
            layout: &self.rect_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: place.as_entire_binding() },
            ],
        })
    }

    /// Put a picture's texture on a transparent canvas-size float texture.
    fn on_canvas(&self, enc: &mut wgpu::CommandEncoder, tex: &wgpu::Texture, rect: Rect, xf: Xf, w: u32, h: u32) -> wgpu::Texture {
        let canvas = self.work_texture(w, h);
        let group = self.rect_group(&tex.create_view(&Default::default()), rect, xf, w, h);
        let out = canvas.create_view(&Default::default());
        let mut rp = begin(enc, &out, wgpu::Color::TRANSPARENT);
        rp.set_pipeline(&self.rect_work);
        rp.set_bind_group(0, &group, &[]);
        rp.draw(0..6, 0..1);
        drop(rp);
        canvas
    }

    /// Render a frame: `items` bottom to top onto opaque black, `width x height`.
    /// Returns RGBA8, sRGB-encoded, opaque.
    pub fn render(&self, width: u32, height: u32, items: &[Item<'_>]) -> Result<Vec<u8>> {
        if width == 0 || height == 0 {
            return Err(RenderError::Invalid("output size must be at least 1x1".into()));
        }
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        // Textures to draw last, with their rectangles.
        let mut draws: Vec<(wgpu::Texture, Rect, Xf)> = Vec::with_capacity(items.len());
        for item in items {
            match item {
                Item::Picture(p) => {
                    let t = self.picture(&mut enc, p)?;
                    draws.push((t, p.rect, p.xf));
                }
                Item::Transition { outgoing, incoming, pass } => {
                    let a = self.picture(&mut enc, outgoing)?;
                    let b = self.picture(&mut enc, incoming)?;
                    let ca = self.on_canvas(&mut enc, &a, outgoing.rect, outgoing.xf, width, height);
                    let cb = self.on_canvas(&mut enc, &b, incoming.rect, incoming.xf, width, height);
                    let out = self.work_texture(width, height);
                    let pipe = self.fx_pipeline(&pass.fx).map_err(|e| RenderError::Invalid(e.0))?;
                    let (va, vb, vo) = (ca.create_view(&Default::default()), cb.create_view(&Default::default()), out.create_view(&Default::default()));
                    self.encode_pass(&mut enc, pass, &pipe, (&va, Some(&vb)), &vo, (width, height));
                    draws.push((out, Rect { x: 0.0, y: 0.0, w: width as f32, h: height as f32 }, Xf::default()));
                }
            }
        }
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("output"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: OUT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let groups: Vec<wgpu::BindGroup> = draws.iter().map(|(t, r, xf)| self.rect_group(&t.create_view(&Default::default()), *r, *xf, width, height)).collect();
        {
            let view = target.create_view(&Default::default());
            let mut rp = begin(&mut enc, &view, wgpu::Color::BLACK);
            rp.set_pipeline(&self.rect_out);
            for g in &groups {
                rp.set_bind_group(0, g, &[]);
                rp.draw(0..6, 0..1);
            }
        }
        let bytes = self.read(enc, &target, width, height, 4, None)?;
        Ok(bytes)
    }

    /// Stack plain `layers` (bottom first) onto opaque black.
    pub fn composite(&self, width: u32, height: u32, layers: &[Layer<'_>]) -> Result<Vec<u8>> {
        let items: Vec<Item<'_>> = layers
            .iter()
            .map(|l| Item::Picture(Picture { rgba: l.rgba, width: l.width, height: l.height, rect: l.rect, xf: Xf::default(), effects: Vec::new() }))
            .collect();
        self.render(width, height, &items)
    }

    /// Copy a texture back to memory (tightly packed rows of `bpp` bytes).
    fn read(&self, mut enc: wgpu::CommandEncoder, tex: &wgpu::Texture, width: u32, height: u32, bpp: u32, timeout: Option<Duration>) -> Result<Vec<u8>> {
        let row = width * bpp;
        let padded = row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(height) },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        let index = self.queue.submit([enc.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::Wait { submission_index: Some(index), timeout }).map_err(gpu_err)?;
        rx.recv().map_err(gpu_err)?.map_err(gpu_err)?;
        let data = readback.get_mapped_range(..).map_err(gpu_err)?;
        let mut out = Vec::with_capacity((row * height) as usize);
        for r in 0..height as usize {
            let start = r * padded as usize;
            out.extend_from_slice(&data[start..start + row as usize]);
        }
        drop(data);
        readback.unmap();
        Ok(out)
    }

    /// Compile an effect and render three small test frames (start, middle,
    /// end) with its default params. Fails if the GPU rejects it, if a frame
    /// takes longer than 2 s, or if the output has NaN or infinite values.
    pub fn test_fx(&self, fx: &Fx) -> std::result::Result<(), FxError> {
        let pipe = self.fx_pipeline(fx)?;
        const S: u32 = 64;
        // Two different gradients as inputs.
        let grad = |flip: bool| -> Vec<u8> {
            (0..S * S)
                .flat_map(|i| {
                    let (x, y) = ((i % S) as u8 * 4, (i / S) as u8 * 4);
                    if flip { [y, x, 255 - x, 255] } else { [x, y, 128, 255] }
                })
                .collect()
        };
        let (ga, gb) = (grad(false), grad(true));
        let defaults = fx.pack(&serde_json::Map::new(), &indexmap::IndexMap::new(), ve_model::time::Q::from_integer(0));
        for progress in [0.0f32, 0.5, 1.0] {
            let pass = Pass { fx: Arc::new(fx.clone()), params: defaults.clone(), time: progress, progress, images: fx.images_for(&serde_json::Map::new()) };
            let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("fx test") });
            let pic = |rgba| Picture { rgba, width: S, height: S, rect: Rect { x: 0.0, y: 0.0, w: S as f32, h: S as f32 }, xf: Xf::default(), effects: Vec::new() };
            let a = self.picture(&mut enc, &pic(&ga)).map_err(|e| FxError(e.to_string()))?;
            let b = self.picture(&mut enc, &pic(&gb)).map_err(|e| FxError(e.to_string()))?;
            let out = self.work_texture(S, S);
            let (va, vb, vo) = (a.create_view(&Default::default()), b.create_view(&Default::default()), out.create_view(&Default::default()));
            let second = (fx.kind() == Kind::Transition).then_some(&vb);
            self.encode_pass(&mut enc, &pass, &pipe, (&va, second), &vo, (S, S));
            let bytes = self.read(enc, &out, S, S, 8, Some(TEST_TIMEOUT)).map_err(|e| match e {
                RenderError::Io(m) if m.contains("imeout") => FxError(format!(
                    "{}: a 64x64 test frame took longer than {} s; the shader probably loops too long",
                    fx.manifest.shader,
                    TEST_TIMEOUT.as_secs()
                )),
                e => FxError(e.to_string()),
            })?;
            let bad = bytes.chunks_exact(2).map(|b| f16_to_f32(u16::from_le_bytes([b[0], b[1]]))).filter(|v| !v.is_finite()).count();
            if bad > 0 {
                return Err(FxError(format!(
                    "{}: the test frame at progress {progress} has {bad} NaN or infinite values (division by zero, sqrt or log of a negative?)",
                    fx.manifest.shader
                )));
            }
        }
        Ok(())
    }
}

fn begin<'a>(enc: &'a mut wgpu::CommandEncoder, view: &'a wgpu::TextureView, clear: wgpu::Color) -> wgpu::RenderPass<'a> {
    enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: None,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(clear), store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::{Manifest, Origin, Registry};

    fn solid(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
        px.iter().copied().cycle().take((w * h * 4) as usize).collect()
    }

    fn at(img: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * w + x) * 4) as usize;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    fn full(s: u32) -> Rect {
        Rect { x: 0.0, y: 0.0, w: s as f32, h: s as f32 }
    }

    #[test]
    fn stacks_layers_and_blends_in_linear_light() {
        let gpu = Gpu::shared().expect("a GPU or software adapter");
        let red = solid(4, 4, [255, 0, 0, 255]);
        let half_green = solid(2, 2, [0, 255, 0, 128]);
        let top_left = Rect { x: 0.0, y: 0.0, w: 4.0, h: 4.0 };
        let out = gpu
            .composite(8, 8, &[Layer { rgba: &red, width: 4, height: 4, rect: full(8) }, Layer { rgba: &half_green, width: 2, height: 2, rect: top_left }])
            .unwrap();
        assert_eq!(out.len(), 8 * 8 * 4);
        assert_eq!(at(&out, 8, 7, 7), [255, 0, 0, 255], "bottom layer alone");
        // 50% green over red, mixed in linear light: about 188 per channel once encoded, not 128.
        let mixed = at(&out, 8, 1, 1);
        assert!((185..=190).contains(&mixed[0]) && (185..=190).contains(&mixed[1]) && mixed[2] == 0, "{mixed:?}");
        assert_eq!(mixed[3], 255);
    }

    #[test]
    fn crops_rotates_and_fades() {
        let gpu = Gpu::shared().expect("a GPU or software adapter");
        let red = solid(4, 4, [255, 0, 0, 255]);
        let draw = |rect: Rect, xf: Xf| {
            gpu.render(8, 8, &[Item::Picture(Picture { rgba: &red, width: 4, height: 4, rect, xf, effects: Vec::new() })]).unwrap()
        };
        // Left half cut away: it stays where it was, not stretched.
        let out = draw(full(8), Xf { crop: [0.5, 0.0, 0.0, 0.0], ..Xf::default() });
        assert_eq!(at(&out, 8, 1, 4), [0, 0, 0, 255]);
        assert_eq!(at(&out, 8, 6, 4), [255, 0, 0, 255]);
        // A wide strip turned a quarter turn becomes a tall one.
        let strip = Rect { x: 0.0, y: 2.0, w: 8.0, h: 4.0 };
        let out = draw(strip, Xf::default());
        assert_eq!((at(&out, 8, 4, 0), at(&out, 8, 0, 4)), ([0, 0, 0, 255], [255, 0, 0, 255]));
        let out = draw(strip, Xf { rotation: std::f32::consts::FRAC_PI_2, ..Xf::default() });
        assert_eq!((at(&out, 8, 4, 0), at(&out, 8, 0, 4)), ([255, 0, 0, 255], [0, 0, 0, 255]));
        // Half opacity over black, in linear light: about 188 once encoded.
        let out = draw(full(8), Xf { opacity: 0.5, ..Xf::default() });
        let px = at(&out, 8, 4, 4);
        assert!((185..=190).contains(&px[0]) && px[1] == 0, "{px:?}");
    }

    #[test]
    fn empty_frame_is_black() {
        let gpu = Gpu::shared().expect("a GPU or software adapter");
        let out = gpu.composite(300, 2, &[]).unwrap();
        assert!(out.chunks(4).all(|p| p == [0, 0, 0, 255]));
    }

    fn pass(fx: &Arc<Fx>, values: serde_json::Value, progress: f32) -> Pass {
        let values = values.as_object().cloned().unwrap_or_default();
        Pass {
            fx: Arc::clone(fx),
            params: fx.pack(&values, &indexmap::IndexMap::new(), ve_model::time::Q::from_integer(0)),
            time: 0.0,
            progress,
            images: fx.images_for(&values),
        }
    }

    #[test]
    fn effects_and_transitions_run() {
        let gpu = Gpu::shared().unwrap();
        let reg = Registry::for_project(None);
        let red = solid(8, 8, [255, 0, 0, 255]);
        let blue = solid(8, 8, [0, 0, 255, 255]);
        // Desaturate red fully: gray with red's luminance (0.2126 linear, about 127 encoded).
        let desat = reg.get("core.color/desaturate").unwrap();
        let out = gpu
            .render(8, 8, &[Item::Picture(Picture { rgba: &red, width: 8, height: 8, rect: full(8), xf: Xf::default(), effects: vec![pass(&desat, serde_json::json!({}), 0.0)] })])
            .unwrap();
        let px = at(&out, 8, 4, 4);
        assert!(px[0] == px[1] && px[1] == px[2] && (125..=129).contains(&px[0]), "{px:?}");
        // Dissolve red to blue halfway, in linear light: about 188 in each channel.
        let dissolve = reg.get("core.transition/dissolve").unwrap();
        let pic = |rgba| Picture { rgba, width: 8, height: 8, rect: full(8), xf: Xf::default(), effects: Vec::new() };
        let out = gpu.render(8, 8, &[Item::Transition { outgoing: pic(&red), incoming: pic(&blue), pass: pass(&dissolve, serde_json::json!({}), 0.5) }]).unwrap();
        let px = at(&out, 8, 4, 4);
        assert!((185..=190).contains(&px[0]) && px[1] == 0 && (185..=190).contains(&px[2]), "{px:?}");
        // Wipe at 25% from the left: left quarter blue, right red.
        let wipe = reg.get("core.transition/wipe").unwrap();
        let out = gpu
            .render(8, 8, &[Item::Transition { outgoing: pic(&red), incoming: pic(&blue), pass: pass(&wipe, serde_json::json!({"softness": 0}), 0.25) }])
            .unwrap();
        assert_eq!(at(&out, 8, 0, 4), [0, 0, 255, 255]);
        assert_eq!(at(&out, 8, 7, 4), [255, 0, 0, 255]);
    }

    #[test]
    fn every_builtin_passes_its_test_render() {
        let gpu = Gpu::shared().unwrap();
        // Audio effects are ffmpeg chains, tested in `fx::tests`.
        for fx in Registry::for_project(None).list.iter().filter(|f| f.kind() != Kind::Audio) {
            gpu.test_fx(fx).unwrap_or_else(|e| panic!("{}: {e}", fx.manifest.id));
        }
    }

    #[test]
    fn test_render_catches_nan() {
        let gpu = Gpu::shared().unwrap();
        let m = Manifest {
            id: "project.nan".into(),
            version: "1.0.0".into(),
            kind: Kind::Effect,
            runtime: "wgsl".into(),
            name: "NaN".into(),
            description: String::new(),
            shader: "effect.wgsl".into(),
            filter: None,
            params: serde_json::json!({"properties": {}}),
            skill: None,
        };
        let src = "fn effect(uv: vec2<f32>) -> vec4<f32> {\n    let z = u.time * 0.0;\n    return vec4<f32>(z / z);\n}\n";
        let fx = Fx::new(m, src.into(), Origin::Project, None).unwrap();
        let e = gpu.test_fx(&fx).unwrap_err();
        assert!(e.0.contains("NaN"), "{e}");
    }
}
