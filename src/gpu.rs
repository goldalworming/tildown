//! wgpu: satu pipeline quad bertekstur. Teks, rect dan gambar semuanya quad;
//! batch hanya pecah saat tekstur atau clip berganti.

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use winit::window::Window;

use crate::text::{Upload, ATLAS};

pub type Rgba = [f32; 4];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct V {
    p: [f32; 2],
    uv: [f32; 2],
    c: [f32; 4],
    /// 0 = atlas (kanal R = alpha), 1 = gambar RGBA.
    m: f32,
}

struct Batch {
    tex: usize,
    clip: [u32; 4],
    end: u32,
}

const SHADER: &str = r#"
struct U { size: vec2<f32>, pad: vec2<f32> };
@group(0) @binding(0) var<uniform> u: U;
@group(1) @binding(0) var tx: texture_2d<f32>;
@group(1) @binding(1) var sp: sampler;

struct VO {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) c: vec4<f32>,
    @location(2) m: f32,
};

@vertex fn vs(@location(0) p: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) c: vec4<f32>, @location(3) m: f32) -> VO {
    var o: VO;
    o.pos = vec4<f32>(p.x / u.size.x * 2.0 - 1.0, 1.0 - p.y / u.size.y * 2.0, 0.0, 1.0);
    o.uv = uv;
    o.c = c;
    o.m = m;
    return o;
}

@fragment fn fs(i: VO) -> @location(0) vec4<f32> {
    let t = textureSample(tx, sp, i.uv);
    if (i.m < 0.5) {
        return vec4<f32>(i.c.rgb, i.c.a * t.a);
    }
    // Gambar disimpan premultiplied (filter linear tanpa halo gelap di tepi
    // transparan); kembalikan ke straight alpha untuk blend biasa.
    return vec4<f32>(t.rgb / max(t.a, 0.0001), t.a) * i.c;
}
"#;

pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    ubuf: wgpu::Buffer,
    ubg: wgpu::BindGroup,
    tex_layout: wgpu::BindGroupLayout,
    samp_near: wgpu::Sampler,
    samp_lin: wgpu::Sampler,
    /// Indeks 0 = atlas glyph. Slot `None` dipakai ulang oleh gambar berikutnya.
    texs: Vec<Option<(wgpu::Texture, wgpu::BindGroup)>>,
    vbuf: wgpu::Buffer,
    vcap: u64,
    verts: Vec<V>,
    batches: Vec<Batch>,
    clip: [u32; 4],
    /// Debug `MARKDOWN_SHOT=berkas.png`: simpan frame ke-3 lalu keluar.
    shot: Option<(String, u32)>,
}

/// Backend yang dicoba berurutan. Windows: GL dulu — diukur di HD 520, GL
/// 26 MB / jendela 130 ms vs DX12 105 MB / 1.070 ms (PRD §6). DX12 cadangan
/// bila driver GL rusak. Vulkan tidak dicoba: crash di HD 520 (brosql).
/// `WGPU_BACKEND` memaksa satu backend.
#[cfg(not(target_arch = "wasm32"))]
fn backends() -> Vec<wgpu::Backends> {
    if let Some(b) = wgpu::Backends::from_env() {
        return vec![b];
    }
    if cfg!(windows) {
        vec![wgpu::Backends::GL, wgpu::Backends::DX12]
    } else {
        vec![wgpu::Backends::PRIMARY | wgpu::Backends::GL]
    }
}

fn vbuf(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("verts"),
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl Gpu {
    // ponytail: init sinkron di thread UI. Dengan GL di HD 520 jendela tampil
    // ~130 ms; pindahkan ke thread latar (pola brosql `start_gpu_init`) bila
    // backend lain membuatnya melewati target PRD §5.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(window: Arc<Window>) -> Result<Gpu, String> {
        pollster::block_on(Gpu::new_async(window))
    }

    /// Di web harus async: browser tidak boleh diblokir menunggu adapter.
    pub async fn new_async(window: Arc<Window>) -> Result<Gpu, String> {
        // Web: satu backend saja. Kanvas terkunci ke konteks pertama yang dibuat,
        // jadi WebGPU yang adapternya gagal tidak boleh dicoba dulu (Firefox headless).
        #[cfg(target_arch = "wasm32")]
        let list = vec![if wgpu::util::is_browser_webgpu_supported().await {
            wgpu::Backends::BROWSER_WEBGPU
        } else {
            wgpu::Backends::GL
        }];
        #[cfg(not(target_arch = "wasm32"))]
        let list = backends();
        let mut err = String::new();
        for b in list {
            match Gpu::with_backend(window.clone(), b).await {
                Ok(g) => return Ok(g),
                Err(e) => err = format!("{b:?}: {e}"),
            }
        }
        Err(err)
    }

    async fn with_backend(window: Arc<Window>, backends: wgpu::Backends) -> Result<Gpu, String> {
        let size = window.inner_size();
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle().with_env();
        desc.backends = backends;
        let instance = wgpu::Instance::new(desc);
        let surface = instance
            .create_surface(window)
            .map_err(|e| format!("surface: {e}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("adapter: {e}"))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("tildown"),
                // WebGL2 tidak memenuhi limit default WebGPU/native.
                required_limits: if cfg!(target_arch = "wasm32") {
                    wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())
                } else {
                    wgpu::Limits::default()
                },
                // Blok heap kecil. Default `Performance` memesan ~256 MB di awal,
                // dan di iGPU itu langsung jadi RAM proses.
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                ..Default::default()
            })
            .await
            .map_err(|e| format!("device: {e}"))?;
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("surface tidak didukung adapter ini")?;
        // Format non-sRGB: warna dan coverage glyph dipakai apa adanya, sama
        // seperti brosql dan stylegen.
        let caps = surface.get_capabilities(&adapter);
        if let Some(f) = caps.formats.iter().copied().find(|f| !f.is_srgb()) {
            config.format = f;
        }
        config.present_mode = wgpu::PresentMode::Fifo;
        let shot = std::env::var("MARKDOWN_SHOT").ok().filter(|_| !cfg!(target_arch = "wasm32"));
        config.desired_maximum_frame_latency = 2;
        surface.configure(&device, &config);

        let ulayout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(16),
                },
                count: None,
            }],
        });
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
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
            ],
        });
        let ubuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ubg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &ulayout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: ubuf.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&ulayout), Some(&tex_layout)],
            immediate_size: 0,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("quad"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let attrs =
            wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("quad"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<V>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attrs,
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = |filter| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: filter,
                min_filter: filter,
                ..Default::default()
            })
        };
        let (samp_near, samp_lin) = (
            sampler(wgpu::FilterMode::Nearest),
            sampler(wgpu::FilterMode::Linear),
        );
        let vcap = 64 * 1024;
        let vb = vbuf(&device, vcap);
        let mut g = Gpu {
            device,
            queue,
            surface,
            config,
            pipeline,
            ubuf,
            ubg,
            tex_layout,
            samp_near,
            samp_lin,
            texs: Vec::new(),
            vbuf: vb,
            vcap,
            verts: Vec::new(),
            batches: Vec::new(),
            clip: [0; 4],
            shot: shot.map(|p| (p, 0)),
        };
        // RGBA: glyph biasa = putih × coverage, emoji = warna premultiplied.
        g.make_tex(ATLAS, ATLAS, wgpu::TextureFormat::Rgba8Unorm, false);
        Ok(g)
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return; // diminimize: jangan configure ukuran nol
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
    }

    fn make_tex(&mut self, w: u32, h: u32, format: wgpu::TextureFormat, linear: bool) -> usize {
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.tex_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(if linear {
                        &self.samp_lin
                    } else {
                        &self.samp_near
                    }),
                },
            ],
        });
        match self.texs.iter().position(Option::is_none) {
            Some(i) => {
                self.texs[i] = Some((tex, bg));
                i
            }
            None => {
                self.texs.push(Some((tex, bg)));
                self.texs.len() - 1
            }
        }
    }

    fn write(&self, id: usize, x: u32, y: u32, w: u32, h: u32, bpp: u32, data: &[u8]) {
        let Some(Some((tex, _))) = self.texs.get(id) else {
            return;
        };
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * bpp),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn add_image(&mut self, w: u32, h: u32, rgba: &[u8]) -> usize {
        let id = self.make_tex(w, h, wgpu::TextureFormat::Rgba8Unorm, true);
        self.write(id, 0, 0, w, h, 4, rgba);
        id
    }

    pub fn free_image(&mut self, id: usize) {
        if id != 0 && id < self.texs.len() {
            self.texs[id] = None;
        }
    }

    pub fn begin(&mut self) {
        self.verts.clear();
        self.batches.clear();
        self.set_clip(None);
    }

    pub fn set_clip(&mut self, r: Option<Rect>) {
        let (w, h) = (self.config.width as f32, self.config.height as f32);
        let r = r.unwrap_or(Rect::new(0.0, 0.0, w, h));
        let x0 = r.x.clamp(0.0, w) as u32;
        let y0 = r.y.clamp(0.0, h) as u32;
        let x1 = (r.x + r.w).clamp(0.0, w) as u32;
        let y1 = (r.y + r.h).clamp(0.0, h) as u32;
        self.clip = [x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0)];
    }

    pub fn quad(&mut self, tex: usize, r: Rect, uv: [f32; 4], c: Rgba, m: f32) {
        if self.clip[2] == 0 || self.clip[3] == 0 {
            return;
        }
        let (x0, y0, x1, y1) = (r.x, r.y, r.x + r.w, r.y + r.h);
        let [u0, v0, u1, v1] = uv;
        let v = |x, y, u, w| V {
            p: [x, y],
            uv: [u, w],
            c,
            m,
        };
        self.verts.extend_from_slice(&[
            v(x0, y0, u0, v0),
            v(x1, y0, u1, v0),
            v(x0, y1, u0, v1),
            v(x0, y1, u0, v1),
            v(x1, y0, u1, v0),
            v(x1, y1, u1, v1),
        ]);
        let end = self.verts.len() as u32;
        match self.batches.last_mut() {
            Some(b) if b.tex == tex && b.clip == self.clip => b.end = end,
            _ => self.batches.push(Batch {
                tex,
                clip: self.clip,
                end,
            }),
        }
    }

    /// Segitiga berwarna per verteks (tiap 3 verteks satu segitiga), digeser (dx, dy).
    pub fn tris(&mut self, m: &[((f32, f32), Rgba)], dx: f32, dy: f32) {
        if self.clip[2] == 0 || self.clip[3] == 0 || m.is_empty() {
            return;
        }
        let u = 2.0 / ATLAS as f32;
        self.verts.extend(m.iter().map(|&((x, y), c)| V {
            p: [x + dx, y + dy],
            uv: [u, u],
            c,
            m: 0.0,
        }));
        let end = self.verts.len() as u32;
        match self.batches.last_mut() {
            Some(b) if b.tex == 0 && b.clip == self.clip => b.end = end,
            _ => self.batches.push(Batch {
                tex: 0,
                clip: self.clip,
                end,
            }),
        }
    }

    pub fn rect(&mut self, r: Rect, c: Rgba) {
        let u = 2.0 / ATLAS as f32; // tengah blok putih
        self.quad(0, r, [u, u, u, u], c, 0.0);
    }

    pub fn finish(&mut self, clear: Rgba, uploads: &mut Vec<Upload>) {
        for u in uploads.drain(..) {
            self.write(0, u.x, u.y, u.w, u.h, 4, &u.data);
        }
        use wgpu::CurrentSurfaceTexture as Cst;
        let frame = match self.surface.get_current_texture() {
            Cst::Success(f) | Cst::Suboptimal(f) => f,
            Cst::Lost | Cst::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return; // frame dilewati; redraw berikutnya memakai surface baru
            }
            _ => return,
        };
        let size = [
            self.config.width as f32,
            self.config.height as f32,
            0.0,
            0.0,
        ];
        self.queue
            .write_buffer(&self.ubuf, 0, bytemuck::cast_slice(&size));
        let bytes = std::mem::size_of_val(self.verts.as_slice()) as u64;
        if bytes > self.vcap {
            self.vcap = bytes.next_power_of_two();
            self.vbuf = vbuf(&self.device, self.vcap);
        }
        if bytes > 0 {
            self.queue
                .write_buffer(&self.vbuf, 0, bytemuck::cast_slice(&self.verts));
        }
        // Debug shot: frame ke-3 digambar ke tekstur sendiri (surface GL tidak bisa disalin).
        let shoot = self.shot.as_mut().is_some_and(|(_, n)| {
            *n += 1;
            *n == 3
        });
        let off = shoot.then(|| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: frame.texture.size(),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        });
        let view = off.as_ref().unwrap_or(&frame.texture).create_view(&Default::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let [r, g, b, a] = clear.map(f64::from);
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.ubg, &[]);
            pass.set_vertex_buffer(0, self.vbuf.slice(..));
            let mut start = 0;
            for b in &self.batches {
                if let Some(Some((_, bg))) = self.texs.get(b.tex) {
                    pass.set_bind_group(1, bg, &[]);
                    pass.set_scissor_rect(b.clip[0], b.clip[1], b.clip[2], b.clip[3]);
                    pass.draw(start..b.end, 0..1);
                }
                start = b.end;
            }
        }
        self.queue.submit(Some(enc.finish()));
        if let (Some(t), Some((path, _))) = (&off, &self.shot) {
            self.save_shot(t, path);
            std::process::exit(0);
        }
        self.queue.present(frame);
    }

    /// Salin tekstur frame ke PNG (debug, lihat `shot`).
    fn save_shot(&self, tex: &wgpu::Texture, path: &str) {
        let (w, h) = (self.config.width, self.config.height);
        let row = (w * 4).div_ceil(256) * 256;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            tex.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(enc.finish()));
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
        let Ok(data) = buf.slice(..).get_mapped_range() else {
            return;
        };
        let bgr = matches!(
            self.config.format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        );
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let i = (y * row + x * 4) as usize;
                let c = &data[i..i + 4];
                px.extend_from_slice(&if bgr { [c[2], c[1], c[0], 255] } else { [c[0], c[1], c[2], 255] });
            }
        }
        if let Err(e) = image::save_buffer(path, &px, w, h, image::ColorType::Rgba8) {
            eprintln!("shot: {e}");
        }
    }
}
