use crate::{map_nes_err, nes::event::FrameEvent, NesResult};
use pix_engine::event::PixEvent;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::thread;

const MAX_PEERS: usize = 1;

#[derive(Clone)]
pub(super) struct NetworkStream {
    event_queue: Vec<FrameEvent>,
}

impl NetworkStream {
    pub(super) fn new(peers: Option<Vec<String>>) -> NesResult<Self> {
        let handle = thread::spawn(|| -> NesResult<()> {
            let listener = TcpListener::bind("127.0.0.1:7878")?;

            for stream in listener.incoming() {
                let stream = stream?;

                println!("Connection established!");
                Self::handle_connection(stream);
            }
            Ok(())
        });

        Ok(Self {
            event_queue: Vec::new(),
        })
    }

    fn handle_connection(mut stream: TcpStream) {
        let mut buffer = [0; 512];
        stream.read(&mut buffer).unwrap();

        let response = "HTTP/1.1 200 OK\r\n\r\n";

        stream.write(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    }

    /// Drains received events for this frame out to subscriber
    pub(super) fn poll(&mut self, frame: usize) -> Vec<PixEvent> {
        // if let Some(frame_event) = self.received_events.pop() {
        //     if frame_event.frame == frame {
        //         return frame_event.events;
        //     }
        // }
        Vec::new()
    }

    pub(super) fn enqueue(&mut self, frame_event: FrameEvent) {
        self.event_queue.push(frame_event);
    }
}
