//! Phase 2 compositor (spike): stacks decoded frames on the GPU with wgpu.
//!
//! Layers arrive as RGBA8 with straight alpha, sRGB-encoded (what the
//! decoder gives). They are uploaded as `Rgba8UnormSrgb` textures, so the
//! GPU samples them as linear light; the fragment shader premultiplies; and
//! the target is also `Rgba8UnormSrgb`, so blending happens in linear light
//! and the result is encoded back on write. Layers draw bottom to top with
//! premultiplied "over" onto opaque black.
//!
//! Not here yet: effect passes (the WGSL effect contract), transforms beyond
//! placing a layer in a rectangle, Rec.709 vs sRGB transfer differences, and
//! a texture cache. Each frame uploads its layers fresh.

use std::sync::OnceLock;

use wgpu::util::DeviceExt as _;

use crate::RenderError;

type Result<T> = std::result::Result<T, RenderError>;

/// One picture to place on the output.
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

const SHADER: &str = r"
struct Place {
    // Destination rectangle in normalized device coordinates: x0, y0, x1, y1.
    rect: vec4<f32>,
};

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> place: Place;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    // Two triangles covering the rectangle.
    var corners = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
    );
    let c = corners[i];
    let r = place.rect;
    var out: VsOut;
    out.pos = vec4(mix(r.x, r.z, c.x), mix(r.y, r.w, c.y), 0.0, 1.0);
    out.uv = c;
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, in.uv);
    return vec4(c.rgb * c.a, c.a);
}
";

/// A GPU device with the compositing pipeline built.
pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
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

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

impl Gpu {
    /// The process-wide GPU, created on first use. Prefers a discrete GPU and
    /// falls back to any adapter, including a software one (WARP, llvmpipe).
    pub fn shared() -> Result<&'static Gpu> {
        static GPU: OnceLock<std::result::Result<Gpu, String>> = OnceLock::new();
        GPU.get_or_init(|| Gpu::new().map_err(|e| e.to_string())).as_ref().map_err(|e| RenderError::Io(e.clone()))
    }

    pub fn new() -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        }))
        .or_else(|_| {
            block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: true,
                compatible_surface: None,
                ..Default::default()
            }))
        })
        .map_err(|e| gpu_err(format!("no usable adapter: {e}")))?;
        let info = adapter.get_info();
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("ve compositor"),
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .map_err(gpu_err)?;

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("composite"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("layer"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("composite"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("composite"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("layer"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Ok(Self { device, queue, pipeline, layout, sampler, adapter: format!("{} ({:?})", info.name, info.backend) })
    }

    /// Stack `layers` (bottom first) onto opaque black at `width x height`.
    /// Returns RGBA8, sRGB-encoded, opaque.
    pub fn composite(&self, width: u32, height: u32, layers: &[Layer<'_>]) -> Result<Vec<u8>> {
        if width == 0 || height == 0 {
            return Err(RenderError::Invalid("output size must be at least 1x1".into()));
        }
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("output"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());

        let mut groups = Vec::with_capacity(layers.len());
        for l in layers {
            if l.rgba.len() != (l.width as usize) * (l.height as usize) * 4 {
                return Err(RenderError::Invalid(format!("layer is {}x{} but has {} bytes", l.width, l.height, l.rgba.len())));
            }
            let size = wgpu::Extent3d { width: l.width, height: l.height, depth_or_array_layers: 1 };
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("layer"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                l.rgba,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(l.width * 4), rows_per_image: Some(l.height) },
                size,
            );
            // Pixel rect to NDC (y up).
            let (w, h) = (width as f32, height as f32);
            let ndc = [
                l.rect.x / w * 2.0 - 1.0,
                1.0 - l.rect.y / h * 2.0,
                (l.rect.x + l.rect.w) / w * 2.0 - 1.0,
                1.0 - (l.rect.y + l.rect.h) / h * 2.0,
            ];
            let bytes: Vec<u8> = ndc.iter().flat_map(|v| v.to_le_bytes()).collect();
            let place = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("place"),
                contents: &bytes,
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            groups.push(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("layer"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: place.as_entire_binding() },
                ],
            }));
        }

        // Rows in a texture-to-buffer copy are padded to 256 bytes.
        let row = width * 4;
        let padded = row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("composite") });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            for g in &groups {
                pass.set_bind_group(0, g, &[]);
                pass.draw(0..6, 0..1);
            }
        }
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(height) },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);

        let (tx, rx) = std::sync::mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely()).map_err(gpu_err)?;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
        px.iter().copied().cycle().take((w * h * 4) as usize).collect()
    }

    fn at(img: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * w + x) * 4) as usize;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    #[test]
    fn stacks_layers_and_blends_in_linear_light() {
        let gpu = Gpu::shared().expect("a GPU or software adapter");
        let red = solid(4, 4, [255, 0, 0, 255]);
        let half_green = solid(2, 2, [0, 255, 0, 128]);
        let full = Rect { x: 0.0, y: 0.0, w: 8.0, h: 8.0 };
        let top_left = Rect { x: 0.0, y: 0.0, w: 4.0, h: 4.0 };
        let out = gpu
            .composite(
                8,
                8,
                &[
                    Layer { rgba: &red, width: 4, height: 4, rect: full },
                    Layer { rgba: &half_green, width: 2, height: 2, rect: top_left },
                ],
            )
            .unwrap();
        assert_eq!(out.len(), 8 * 8 * 4);
        assert_eq!(at(&out, 8, 7, 7), [255, 0, 0, 255], "bottom layer alone");
        // 50% green over red, mixed in linear light: each channel is
        // sqrt-ish of 0.5 once encoded (about 188), not 128.
        let mixed = at(&out, 8, 1, 1);
        assert!((185..=190).contains(&mixed[0]) && (185..=190).contains(&mixed[1]) && mixed[2] == 0, "{mixed:?}");
        assert_eq!(mixed[3], 255);
    }

    #[test]
    fn empty_frame_is_black() {
        let gpu = Gpu::shared().expect("a GPU or software adapter");
        let out = gpu.composite(300, 2, &[]).unwrap();
        assert!(out.chunks(4).all(|p| p == [0, 0, 0, 255]));
    }
}
