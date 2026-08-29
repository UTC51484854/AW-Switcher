use tray_icon::Icon;

/// Draws a small monochrome monitor glyph so no external icon asset is needed.
/// Works as a macOS "template" image (adapts to light/dark menu bars) and as
/// a plain black-on-transparent icon elsewhere.
pub fn tray_icon() -> Icon {
    const W: u32 = 22;
    const H: u32 = 22;
    let mut rgba = vec![0u8; (W * H * 4) as usize];

    let mut set = |x: i32, y: i32| {
        if x < 0 || y < 0 || x as u32 >= W || y as u32 >= H {
            return;
        }
        let i = ((y as u32 * W + x as u32) * 4) as usize;
        rgba[i] = 0;
        rgba[i + 1] = 0;
        rgba[i + 2] = 0;
        rgba[i + 3] = 255;
    };

    // Screen bezel (rectangle outline).
    for x in 2..20 {
        set(x, 2);
        set(x, 3);
        set(x, 13);
        set(x, 14);
    }
    for y in 2..15 {
        set(2, y);
        set(3, y);
        set(18, y);
        set(19, y);
    }

    // Stand.
    for y in 15..18 {
        set(10, y);
        set(11, y);
    }
    for x in 7..15 {
        set(x, 18);
        set(x, 19);
    }

    Icon::from_rgba(rgba, W, H).expect("static icon buffer is always valid")
}
