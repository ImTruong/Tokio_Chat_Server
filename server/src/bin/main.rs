use std::io::{self, ErrorKind};
use std::net::SocketAddr;
use std::{collections::HashSet, sync::Arc};
use compact_str::CompactString;
use dashmap::{DashMap, DashSet};
use futures::{SinkExt, StreamExt};
use tokio::{net::{TcpListener, TcpStream}, sync::broadcast::{self, Sender, error::RecvError}};
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec, LinesCodecError};
use tracing_appender::rolling::Rotation;
use chat_server::{b, NameGenerator};

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

const MAIN: &str = "main";
// const HELP_MSG: &str = include_str!("help.txt");
const MAX_MSG_LEN: usize = 400;
const ROOM_CHANNEL_CAPACITY: usize = 1024;

struct Names(Arc<DashSet<CompactString>>);

impl Names {
    fn new() -> Self {
        Self(Arc::new(DashSet::new()))
    }
    fn insert(&mut self, name: CompactString) -> bool {
        self.0.insert(name)
    }
    fn remove(&mut self, name: &str) -> bool {
        self.0.remove(name).is_some()
    }
    fn get_unique(&mut self, name_generator: &mut NameGenerator) -> CompactString {
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

}



fn main(){

}