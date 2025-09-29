use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen
};
use futures::{SinkExt, StreamExt};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Layout},
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Terminal,
};
use std::{
    borrow::Cow,
    io,
};
use tokio::net::TcpStream;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};
use tracing_appender::non_blocking::WorkerGuard;
use tui_textarea::{Input, Key, TextArea};

// TODO: Thay đổi tuỳ server, mặc định là localhost
const SERVER_ADD: &str = "127.0.0.1:8080";
// const SERVER_ADDR: &str = "127.0.0.1:8080";

// Khởi tạo textarea
fn textarea_new() -> TextArea<'static> {
    let mut textarea = TextArea::default();
    textarea.set_cursor_style(Style::default());
    textarea.set_placeholder_text("Start typing...");
    textarea.set_block(
        Block::default().borders(Borders::ALL).title("Send message"),
    );
    textarea
}

fn messages_to_list(msgs: &[String], min_lines: usize, max_length: usize) -> List<'_> {
    let mut list_items = Vec::new();

    // Lặp các tin nhắn theo thứ tự ngược -> Lấy tin mới nhất trước
    'outer: for msg in msgs.iter().rev() {
        let user_msg = msg.contains(':');
        // Ngắt dòng đẹp
        let lines = textwrap::wrap(
            msg,
            textwrap::Options::new(max_length)
                .wrap_algorithm(textwrap::WrapAlgorithm::new_optimal_fit()),
        );
        let mut styled_lines = Vec::new();
        if user_msg {
            let mut lines = lines.into_iter();
            // Lấy dòng đầu tiên
            let first_line = lines.next().unwrap();
            let mut parts = first_line.split(':');
            let mut first_styled_line = Vec::new();
            // Tên user -> in đậm
            first_styled_line.push(parts.next().unwrap().to_owned().bold());
            // Nội dung -> giữ nguyên
            for part in parts {
                first_styled_line.push(Span::raw(":"));
                first_styled_line.push(part.to_owned().into());
            }
            styled_lines.push(Line::from(first_styled_line));
            for line in lines {
                styled_lines.push(Line::from(line.into_owned()));
            }
        } else {
            // Nếu là thông báo hệ thống -> làm mờ + in nghiêng
            styled_lines.extend(
                lines
                    .into_iter()
                    .map(|line| line.into_owned().dim().italic().into()),
            );
        }
        // Duyệt các line đã được chỉnh kiểu theo thứ tự ngược -> render tin mới nhất trước
        for line in styled_lines.into_iter().rev() {
            list_items.push(ListItem::new(line));
            if list_items.len() >= min_lines {
                break 'outer;
            }
        }
    }

    while list_items.len() < min_lines {
        list_items.push(ListItem::new(Cow::from("")));
    }
    list_items.reverse();
    List::new(list_items)
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
        let draw_res = term.draw(|f| {
            let chunks = layout.split(f.size());

            let msgs_height = chunks[0].height - 2;
            let msgs_width = chunks[0].width - 2;
            let msgs_title = format!("Room - {current_room}");

            // Biến msgs thành widget List<'_>
            let msgs = messages_to_list(
                &messages,
                msgs_height.into(),
                msgs_width.into(),
            ).block(Block::default().borders(Borders::ALL).title(msgs_title));

            f.render_widget(msgs, chunks[0]);

            f.render_widget(&textarea, chunks[1]);
        });

        match draw_res {
            Ok(_) => (),
            Err(_) => break
        }

        tokio::select! {
            // Sự kiện terminal: nhấn phím,...
            term_event = term_stream.next() => {
                if let Some(event) = term_event {
                    let event = match event {
                        Ok(event) => event,
                        Err(_) => break
                    };

                    match event.into() {
                        // Sự kiện thoát
                        Input {key: Key::Esc, ..} |
                        Input {key: Key::Char('c'), ctrl: true, ..} |
                        Input {key: Key::Char('d'), ctrl: true, ..} => break,

                        // Nhấn phím Enter
                        Input {key: Key::Enter, ..} => {
                            if textarea.is_empty() {
                                continue;
                            }
                            // Gửi tin nhắn lên server
                            for line in textarea.into_lines() {
                                tracing::info!("SENT {line}");
                                match sink.send(line).await {
                                    Ok(_) => (),
                                    Err(_) => break
                                };
                            }
                            textarea = textarea_new();
                        }
                        // Các sự kiện còn lại không xử lý (Backspace, Delete,...)
                        input => {
                            textarea.input_without_shortcuts(input);
                        }
                    }
                } else {
                    break;
                }
            },

            // Nhận tin nhắn
            tcp_event = stream.next() => match tcp_event {
                Some(event) => {
                    let server_msg = match event {
                        Ok(msg) => msg,
                        Err(_) => break
                    };
                    // Đổi tên phòng
                    if server_msg.starts_with("You joined ") {
                        let room_name = server_msg
                            .split_ascii_whitespace()
                            .nth(2)
                            .unwrap();
                        current_room = room_name.to_owned();
                    }
                    tracing::info!("GOT {server_msg}");
                    messages.push(server_msg);
                },
                None => break
            }
        }
    }

    // Tắt raw mode, thoát về màn hình terminal -> Kết thúc
    disable_raw_mode()?;
    crossterm::execute!(term.backend_mut(), LeaveAlternateScreen,)?;
    term.show_cursor()?;
    Ok(())
}
