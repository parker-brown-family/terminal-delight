// A ratatui-image widget, the smallest honest one: ask the terminal what it
// can do, draw one picture in the whole screen, hold it, leave.
use ratatui_image::{picker::Picker, StatefulImage};
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("image path");
    let log = std::env::args().nth(2).unwrap_or_else(|| "/dev/null".into());
    let picker = Picker::from_query_stdio()?;
    let mut f = std::fs::File::create(log)?;
    writeln!(
        f,
        "protocol={:?} font_size={:?}",
        picker.protocol_type(),
        picker.font_size()
    )?;
    let dyn_img = image::ImageReader::open(path)?.decode()?;
    let mut proto = picker.new_resize_protocol(dyn_img);
    let mut terminal = ratatui::init();
    terminal.draw(|frame| {
        frame.render_stateful_widget(StatefulImage::default(), frame.area(), &mut proto);
    })?;
    std::thread::sleep(std::time::Duration::from_millis(1200));
    ratatui::restore();
    Ok(())
}
