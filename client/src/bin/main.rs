use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Layout},
    Terminal,
};
use std::io;
use tokio::net::TcpStream;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};
use tracing_appender::non_blocking::WorkerGuard;
use tui_textarea::TextArea;

// TODO: Thay đổi tuỳ server, mặc định là localhost
const SERVER_ADD: &str = "127.0.0.1:8080";
// const SERVER_ADDR: &str = "127.0.0.1:8080";

// Khởi tạo textarea
fn textarea_new() -> TextArea<'static> {
    let mut textarea = TextArea::default();
    // TODO: Chỉnh sửa textarea
    textarea
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr = SERVER_ADD;

    // Tạo kết nối Tcp đến server
    let mut conn = match TcpStream::connect(addr).await {
        Ok(conn) => conn,
        Err(err) => {
            match err.kind() {
                std::io::ErrorKind::ConnectionRefused => {
                    println!("Connection refused");
                    std::process::exit(1);
                }
                // Gặp lỗi không xác định -> Kệ
                _ => Err(err)?,
            }
        }
    };

    // Tách reader và writer từ stream
    let (reader, writer) = conn.split();
    let mut sink = FramedWrite::new(writer, LinesCodec::new());
    let mut stream = FramedRead::new(reader, LinesCodec::new());

    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    let _guard: Option<WorkerGuard> = None;

    // Kích hoạt raw mode của terminal: Không echo ký tự nhập vào, Không xử lý Ctr+C...
    enable_raw_mode()?;

    // Chuyển màn hình terminal sang màn hình UI
    crossterm::execute!(stdout, EnterAlternateScreen,)?;

    // Tạo backend cho ratatui -> Tạo terminal từ backend đó để vẽ UI
    let backend = CrosstermBackend::new(stdout);
    let mut term = Terminal::new(backend)?;

    let mut textarea = textarea_new();
    // Tạo layout cho UI: Chiếm 100% chiều cao, chiều cao tối thiểu 3 ô
    let layout = Layout::default().constraints(
        [Constraint::Percentage(100), Constraint::Min(3)]
    );

    let mut messages: Vec<String> = Vec::new();
    let mut current_room = "main".to_owned();

    // Tạo eventstream cho phép nhận sự kiện bàn phím như async stream
    let mut term_stream = crossterm::event::EventStream::new();

    loop {
        //     TODO: Vẽ UI và xử lý gửi nhận tin nhắn
        break;
    }

    // Tắt raw mode, thoát về màn hình terminal -> Kết thúc
    disable_raw_mode()?;
    crossterm::execute!(term.backend_mut(), LeaveAlternateScreen,)?;
    term.show_cursor()?;
    Ok(())
}
