//! What a frame costs to put on screen, by how it is drawn (native/windows/PERFORMANCE.md): a window the bar's size,
//! cleared and given a line of text each frame, through
//!   hwnd     the app's software ID2D1HwndRenderTarget (the whole window each frame)
//!   dc-full  a software ID2D1DCRenderTarget bound to the whole client area in WM_PAINT
//!   dc-part  the same, bound to (and invalidating) only the answer's part of the window
//!   cargo run --release -p qcalc-win --example present_bench -- [w_px h_px frames]
#![allow(unsafe_code)]

#[cfg(windows)]
fn main() {
    imp::main();
}

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
mod imp {
    use std::time::Instant;

    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Direct2D::Common::*;
    use windows::Win32::Graphics::Direct2D::*;
    use windows::Win32::Graphics::DirectWrite::*;
    use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::core::w;
    use windows_numerics::Vector2;

    unsafe extern "system" fn proc_(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        unsafe { DefWindowProcW(h, m, w, l) }
    }

    fn private() -> isize {
        use windows::Win32::System::ProcessStatus::*;
        use windows::Win32::System::Threading::GetCurrentProcess;
        let mut c = PROCESS_MEMORY_COUNTERS_EX {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };
        unsafe {
            let _ = K32GetProcessMemoryInfo(
                GetCurrentProcess(),
                (&mut c as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
                c.cb,
            );
        }
        c.PrivateUsage as isize
    }

    fn pct(v: &mut [f64], p: f64) -> f64 {
        v.sort_by(f64::total_cmp);
        v[(((p / 100.0) * v.len() as f64).ceil() as usize).clamp(1, v.len()) - 1]
    }

    pub fn main() {
        let a: Vec<u32> = std::env::args()
            .skip(1)
            .filter_map(|s| s.parse().ok())
            .collect();
        let (w, h, frames) = (
            *a.first().unwrap_or(&1020),
            *a.get(1).unwrap_or(&108),
            *a.get(2).unwrap_or(&400),
        );
        unsafe {
            let inst = GetModuleHandleW(None).unwrap();
            let class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(proc_),
                hInstance: inst.into(),
                lpszClassName: w!("qcalc-present-bench"),
                ..Default::default()
            };
            RegisterClassExW(&class);
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                w!("qcalc-present-bench"),
                w!("present bench"),
                WS_POPUP | WS_VISIBLE,
                200,
                200,
                w as i32,
                h as i32,
                None,
                None,
                Some(inst.into()),
                None,
            )
            .unwrap();
            let f: ID2D1Factory =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).unwrap();
            let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).unwrap();
            let fmt = dw
                .CreateTextFormat(
                    w!("Segoe UI Variable Display"),
                    None,
                    DWRITE_FONT_WEIGHT_REGULAR,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    36.0,
                    w!("en-us"),
                )
                .unwrap();
            let props = |dpi: f32| D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: dpi,
                dpiY: dpi,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let pump = || {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            };
            let draw = |rt: &ID2D1RenderTarget, k: u32, x0: f32| {
                let brush = rt
                    .CreateSolidColorBrush(
                        &D2D1_COLOR_F {
                            r: 0.1,
                            g: 0.1,
                            b: 0.1,
                            a: 1.0,
                        },
                        None,
                    )
                    .unwrap();
                rt.Clear(Some(&D2D1_COLOR_F {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                }));
                let text: Vec<u16> = format!("sin(90) + {k}").encode_utf16().collect();
                let layout = dw.CreateTextLayout(&text, &fmt, 600.0, h as f32).unwrap();
                rt.DrawTextLayout(
                    Vector2 {
                        X: 20.0 - x0,
                        Y: 20.0,
                    },
                    &layout,
                    &brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                );
            };
            pump();

            // hwnd
            let hp = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd,
                pixelSize: D2D_SIZE_U {
                    width: w,
                    height: h,
                },
                presentOptions: D2D1_PRESENT_OPTIONS_IMMEDIATELY,
            };
            let rt = f.CreateHwndRenderTarget(&props(96.0), &hp).unwrap();
            let mut t_hwnd = vec![];
            for k in 0..frames {
                pump();
                let t = Instant::now();
                rt.BeginDraw();
                draw(&rt, k, 0.0);
                let _ = rt.EndDraw(None, None);
                t_hwnd.push(t.elapsed().as_secs_f64() * 1e6);
            }
            drop(rt);

            // dc, full and part
            let mut dcp = props(96.0);
            dcp.pixelFormat.alphaMode = D2D1_ALPHA_MODE_IGNORE;
            let dc = f.CreateDCRenderTarget(&dcp).unwrap();
            let mut results = vec![];
            for (name, part) in [("dc-full", false), ("dc-part", true)] {
                let mut v = vec![];
                for k in 0..frames {
                    pump();
                    let t = Instant::now();
                    let r = if part {
                        RECT {
                            left: (w * 2 / 3) as i32,
                            top: 0,
                            right: w as i32,
                            bottom: h as i32,
                        }
                    } else {
                        RECT {
                            left: 0,
                            top: 0,
                            right: w as i32,
                            bottom: h as i32,
                        }
                    };
                    let hdc = GetDC(Some(hwnd));
                    dc.BindDC(hdc, &r).unwrap();
                    dc.BeginDraw();
                    draw(&dc, k, r.left as f32);
                    let _ = dc.EndDraw(None, None);
                    ReleaseDC(Some(hwnd), hdc);
                    v.push(t.elapsed().as_secs_f64() * 1e6);
                }
                results.push((name, v));
            }
            results.insert(0, ("hwnd", t_hwnd));

            // a D2D device context on a WARP D3D11 device, into a flip-model swap chain; Present1 full or dirty
            {
                use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_WARP;
                use windows::Win32::Graphics::Direct3D11::*;
                use windows::Win32::Graphics::Dxgi::Common::*;
                use windows::Win32::Graphics::Dxgi::*;
                use windows::core::Interface;
                let before = private();
                let mut dev = None;
                D3D11CreateDevice(
                    None,
                    D3D_DRIVER_TYPE_WARP,
                    windows::Win32::Foundation::HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    None,
                    D3D11_SDK_VERSION,
                    Some(&mut dev),
                    None,
                    None,
                )
                .unwrap();
                let dev: ID3D11Device = dev.unwrap();
                let dxgi_dev: IDXGIDevice = dev.cast().unwrap();
                let f1: ID2D1Factory1 = f.cast().unwrap();
                let d2d_dev = f1.CreateDevice(&dxgi_dev).unwrap();
                let ctx = d2d_dev
                    .CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)
                    .unwrap();
                let adapter = dxgi_dev.GetAdapter().unwrap();
                let factory: IDXGIFactory2 = adapter.GetParent().unwrap();
                let desc = DXGI_SWAP_CHAIN_DESC1 {
                    Width: w,
                    Height: h,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                    BufferCount: 2,
                    SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                    AlphaMode: DXGI_ALPHA_MODE_IGNORE,
                    ..Default::default()
                };
                let sc = factory
                    .CreateSwapChainForHwnd(&dev, hwnd, &desc, None, None)
                    .unwrap();
                let surface: IDXGISurface = sc.GetBuffer(0).unwrap();
                let bp = D2D1_BITMAP_PROPERTIES1 {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_IGNORE,
                    },
                    dpiX: 96.0,
                    dpiY: 96.0,
                    bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                    ..Default::default()
                };
                let _ = &surface;
                let mut made = private() - before;
                for (name, part) in [("warp-sc", false), ("warp-sc-dirty", true)] {
                    let mut v = vec![];
                    for k in 0..frames {
                        pump();
                        let t = Instant::now();
                        // flip model: the back buffer changes each present
                        let surface: IDXGISurface = sc.GetBuffer(0).unwrap();
                        let bmp = ctx
                            .CreateBitmapFromDxgiSurface(&surface, Some(&bp))
                            .unwrap();
                        ctx.SetTarget(&bmp);
                        ctx.BeginDraw();
                        draw(&ctx, k, 0.0);
                        let _ = ctx.EndDraw(None, None);
                        ctx.SetTarget(None);
                        let mut r = RECT {
                            left: (w * 2 / 3) as i32,
                            top: 0,
                            right: w as i32,
                            bottom: h as i32,
                        };
                        let params = DXGI_PRESENT_PARAMETERS {
                            DirtyRectsCount: if part && k > 1 { 1 } else { 0 },
                            pDirtyRects: &mut r,
                            ..Default::default()
                        };
                        let _ = sc.Present1(0, DXGI_PRESENT(0), &params);
                        v.push(t.elapsed().as_secs_f64() * 1e6);
                    }
                    made = made.max(private() - before);
                    results.push((name, v));
                }
                println!("warp swap chain: +{:.2} MB private", made as f64 / 1e6);
            }
            for (name, mut v) in results {
                println!(
                    "{name:>8}: {w}x{h} px, {frames} frames: median {:.1} us p90 {:.1} p99 {:.1}",
                    pct(&mut v, 50.0),
                    pct(&mut v, 90.0),
                    pct(&mut v, 99.0)
                );
            }
            let _ = DestroyWindow(hwnd);
        }
    }
}
