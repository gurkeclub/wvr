use std::sync::mpsc::channel;
use std::thread;

use anyhow::{Context, Result};

use glium::glutin::event_loop::EventLoop;

use wvr_com::server::OrderServer;

use wvr::{start_wvr, Wvr};

fn main() -> Result<()> {
    if let Err(err) = wvr::utils::init_wvr_data_directory() {
        eprintln!("{:?}", err);
    }

    let (project_path, config) = wvr::utils::get_config()?;

    let (order_sender, order_receiver) = channel();
    if config.server.enable {
        let mut order_server = OrderServer::new(&config.server);
        thread::spawn(move || loop {
            order_sender
                .send(order_server.next_order(None).unwrap())
                .unwrap();
        });
    }
    let event_loop = EventLoop::new();

    let app = Wvr::new(&project_path, config, &event_loop, order_receiver)
        .context("Failed creating Wvr app")?;

    start_wvr(app, event_loop);

    Ok(())
}
