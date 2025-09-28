use std::io::{self, ErrorKind};
use std::net::SocketAddr;
use std::{collections::HashSet, sync::Arc};
use compact_str::CompactString;
use dashmap::{DashMap, DashSet};
use futures::{SinkExt, StreamExt};
use tokio::{net::{TcpListener, TcpStream}, sync::broadcast::{self, Sender, error::RecvError}};
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec, LinesCodecError};
use tracing_appender::rolling::Rotation;
use chat_server::{b, NameGenerator, valid_name};

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

const MAIN: &str = "main";
const HELP_MSG: &str = include_str!("help.txt");
const MAX_MSG_LEN: usize = 400;
const ROOM_CHANNEL_CAPACITY: usize = 1024;

#[derive(Clone)]
#[repr(transparent)]
struct Names(Arc<DashSet<CompactString>>);

impl Names {
    fn new() -> Self {
        Self(Arc::new(DashSet::with_capacity(32)))
    }
    fn insert(&self, name: CompactString) -> bool {
        self.0.insert(name)
    }
    fn remove(&self, name: &str) -> bool {
        self.0.remove(name).is_some()
    }
    fn get_unique(&self, name_generator: &mut NameGenerator) -> CompactString {
        let mut name = name_generator.next();
        while !self.0.insert(name.clone()) {
            name = name_generator.next();
        }
        name
    }
}

#[derive(Clone)]
enum RoomMsg{
    Joined(CompactString),
    Left(CompactString),
    Msg(Arc<str>),
}

struct Room {
    tx: Sender<RoomMsg>,
    users: HashSet<CompactString>,
}

impl Room {
    fn new() -> Self {
        let (tx,_) = broadcast::channel(ROOM_CHANNEL_CAPACITY);
        let users = HashSet::with_capacity(8);
        Self { tx, users }
    }
}

#[derive(Clone)]
#[repr(transparent)]
struct Rooms(Arc<DashMap<CompactString, Room>>);

impl Rooms {
    fn new() -> Self {
        Self(Arc::new(DashMap::new()))
    }

    fn join(&self, room_name: &str, user_name: &str) -> Sender<RoomMsg> {
        let mut room = self.0.entry(room_name.into()).or_insert(Room::new());
        room.users.insert(user_name.into());
        room.tx.clone()
    }

    fn leave(&self, room_name: &str, user_name: &str) {
        let mut delete_room = false;
        if let Some(mut room) = self.0.get_mut(room_name) {
            room.users.remove(user_name);
            delete_room = room.tx.receiver_count() <= 1;
        }
        if delete_room {
            self.0.remove(room_name);
        }
    }

    fn change(&self, prev_room: &str, next_room: &str, user_name: &str) -> Sender<RoomMsg> {
        self.leave(prev_room, user_name);
        self.join(next_room, user_name)
    }

    fn change_name(&self, room_name: &str, prev_name: &str, next_name: &str) {
        if let Some(mut room) = self.0.get_mut(room_name) {
            self.leave(prev_name, room_name);
            self.join(next_name, room_name);
        }
    }

    fn list(&self) -> Vec<(CompactString,usize)> {
        let mut list: Vec<_> = self
            .0
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().tx.receiver_count()))
            .collect();
        list.sort_by(|a, b| {
            use std::cmp::Ordering;
            match b.1.cmp(&a.1) {
                Ordering::Equal => a.0.cmp(&b.0),
                ordering => ordering,
            }
        });
        list
    }

    fn list_users(&self, room_name: &str) -> Option<Vec<CompactString>> {
        self.0.get(room_name).map(|room| room.users.iter().cloned().collect())
    }

}

#[tokio::main]
async fn main() -> Result<(), io::Error> {
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let server = TcpListener::bind(addr).await?;
    let mut name_generator = NameGenerator::new();
    let names = Names::new();
    let rooms = Rooms::new();
    loop {
        let (tcp, _ ) = server.accept().await?;
        let unique_name = names.get_unique(&mut name_generator);
        tokio::spawn(handle_user(tcp, names.clone(), rooms.clone(), unique_name));
    }
}

async fn handle_user(
    mut tcp: TcpStream,
    names: Names,
    rooms: Rooms,
    mut name: CompactString,
){
    let (reader,writer) = tcp.split();
    let mut stream = FramedRead::new(reader, LinesCodec::new_with_max_length(MAX_MSG_LEN));
    let mut sink = FramedWrite::new(writer, LinesCodec::new_with_max_length(MAX_MSG_LEN + 100));
    let mut exit_result = sink.send(format!("{HELP_MSG}\nYou are {name}")).await;
    if should_exit(exit_result){
        names.remove(&name);
        return;
    }
    let mut room_name = CompactString::from(MAIN);
    let mut room_tx = rooms.join(&room_name, &name);
    let mut room_rx = room_tx.subscribe();
    let _ = room_tx.send(RoomMsg::Joined(name.clone()));
    let mut discarding_long_msg = false;
    exit_result = loop {
        tokio::select! {
            user_msg = stream.next() => {
                let user_msg = match user_msg {
                    Some(msg) => match msg{
                        Ok(ok) => ok,
                        Err(LinesCodecError::MaxLineLengthExceeded) => {
                            b!(sink.send(format!("Messages can only be {MAX_MSG_LEN} chars long")).await);
                            discarding_long_msg = true;
                            continue;
                        },
                        Err(LinesCodecError::Io(io_err)) => {
                            match io_err.kind() {
                                // user typed invalid utf8 like ^C or ^D
                                // and is probably trying to quit
                                ErrorKind::InvalidData | ErrorKind::InvalidInput => {
                                    break Ok(());
                                },
                                // user disconnected
                                ErrorKind::BrokenPipe | ErrorKind::ConnectionReset => {
                                    break Ok(());
                                },
                                // unexpected err, re-throw it
                                _ => break Err(LinesCodecError::Io(io_err)),
                            }
                        }
                    },
                    None => {
                        if !discarding_long_msg {
                            break Ok(());
                        }
                        discarding_long_msg = false;
                        continue;
                    }
                };
                if user_msg.starts_with("/help"){
                    b!(sink.send(HELP_MSG).await);
                } else if user_msg.starts_with("/name"){
                    let new_name = user_msg
                        .split_ascii_whitespace()
                        .nth(1);
                    if !valid_name(new_name){
                        b!(sink.send("Name must be 2 - 20 alphanumeric chars").await);
                        continue;
                    }
                    let new_name = CompactString::from(new_name.unwrap());
                    let changed_name = names.insert(new_name.clone());
                } else if user_msg.starts_with("/join") {
                    let new_room = user_msg
                        .split_ascii_whitespace()
                        .nth(1);
                    if !valid_name(new_room) {
                        b!(sink.send("Room must be 2 - 20 alphanumeric chars").await);
                        continue;
                    }
                    let new_room = CompactString::from(new_room.unwrap());
                    if new_room == room_name {
                        b!(sink.send(format!("You are in {room_name}")).await);
                        continue;
                    }
                    let _ = room_tx.send(RoomMsg::Left(name.clone()));
                    room_tx = rooms.change(&room_name, &new_room, &name);
                    room_rx = room_tx.subscribe();
                    room_name = new_room;
                    let _ = room_tx.send(RoomMsg::Joined(name.clone()));
                } else if user_msg.starts_with("/rooms") {
                    let rooms_list = rooms.list();
                    let mut rooms_msg = String::with_capacity(rooms_list.len() * 15);
                    rooms_msg.push_str("Rooms - ");
                    for room in rooms_list {
                        rooms_msg.push_str(&room.0);
                        rooms_msg.push_str(" (");
                        rooms_msg.push_str(&room.1.to_string());
                        rooms_msg.push_str("), ");
                    }
                    // pop off trailing comma + space
                    rooms_msg.pop();
                    rooms_msg.pop();
                    b!(sink.send(rooms_msg).await);
                } else if user_msg.starts_with("/users") {
                    let users_list = rooms.list_users(&room_name).unwrap();
                    let mut users_msg = String::with_capacity(users_list.len() * 15);
                    users_msg.push_str("Users - ");
                    for user in users_list {
                        users_msg.push_str(&user);
                        users_msg.push_str(", ");
                    }
                    // pop off trailing comma + space
                    users_msg.pop();
                    users_msg.pop();
                    b!(sink.send(users_msg).await);
                } else if user_msg.starts_with("/quit") {
                    break Ok(());
                } else if user_msg.starts_with("/") {
                    let unrecognized = user_msg
                        .split_ascii_whitespace()
                        .next()
                        .unwrap();
                    b!(sink.send(format!("Unrecognized command {unrecognized}, try /help")).await);
                } else {
                    let msg = format!("{name}: {user_msg}");
                    let msg: Arc<str> = Arc::from(msg.as_str());
                    let _ = room_tx.send(RoomMsg::Msg(msg));
                }
            },

        }
    }
}

const IGNORE_KINDS: [ErrorKind; 2] = [ErrorKind::BrokenPipe, ErrorKind::ConnectionReset];

fn should_exit(result: Result<(), LinesCodecError>) -> bool{
    fn ignore(io_err: &io::Error) -> bool {
        IGNORE_KINDS.contains(&io_err.kind())
    }
    match result {
        Ok(_) => false,
        Err(LinesCodecError::MaxLineLengthExceeded) => true,
        Err(LinesCodecError::Io(err)) if ignore(&err) => true,
        Err(LinesCodecError::Io(err)) => {
            tracing::error!("unexpected error: {err}");
            true
        }
    }
}