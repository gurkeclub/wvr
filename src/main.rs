use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::channel,
    Arc,
};
use std::thread;

use anyhow::{Context, Result};

use glium::glutin::event_loop::EventLoop;

use wvr_com::{data::Message, server::OrderServer};

use wvr::{start_wvr, Wvr};

fn main() -> Result<()> {
    if let Err(err) = wvr::utils::init_wvr_data_directory() {
        eprintln!("{:?}", err);
    }

    let (project_path, config) = wvr::utils::get_config()?;

    let play_state = Arc::new(AtomicBool::new(true));
    let (order_sender, order_receiver) = channel();
    if config.server.enable {
        if let Ok(mut order_server) = OrderServer::new(&config.server) {
            let play_state = play_state.clone();

            thread::spawn(move || {
                while play_state.load(Ordering::Relaxed) {
                    if let Some(message) = order_server.next_order(None) {
                        order_sender.send(message).unwrap();
                    }
                }
            });
        }
    } else {
        order_sender.send(Message::Start)?;
    }
    let event_loop = EventLoop::new();

    let window = wvr::utils::build_window(&config.view, &event_loop)?;

    let app = Wvr::new(&project_path, config, &window).context("Failed creating Wvr app")?;

    start_wvr(window, app, event_loop, order_receiver);

    play_state.store(false, Ordering::Relaxed);

    Ok(())
}
