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



fn main(){

}