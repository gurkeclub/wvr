use std::sync::mpsc::channel;
use std::thread;

use anyhow::{Context, Result};

use glium::glutin;
use glium::glutin::event::Event;
use glium::glutin::event_loop::ControlFlow;
use glium::glutin::event_loop::EventLoop;
use glutin::event::WindowEvent;

use wvr_com::data::{Message, SetInfo};
use wvr_com::server::OrderServer;

use wvr::{input_from_config, VBoij};

fn main() -> Result<()> {
    let config = wvr::get_config()?;

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

    let mut app = VBoij::new(config, &event_loop).context("Failed creating VBoij app")?;

    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::WindowEvent { event, .. } => {
                if let WindowEvent::CloseRequested = event {
                    *control_flow = ControlFlow::Exit;
                    return;
                } else if let WindowEvent::Focused(focused) = event {
                    app.set_focused(focused);
                } else if let WindowEvent::CursorMoved { position, .. } = event {
                    app.set_mouse_position((position.x, position.y));
                } else {
                    //println!("{:?}", event);
                }
            }
            Event::RedrawRequested(_) => {
                if let Err(error) = app.update() {
                    eprintln!("Failed to update app: {:?}", error);

                    *control_flow = ControlFlow::Exit;
                    return;
                }

                if let Err(error) = app.render() {
                    eprintln!("Failed to update app: {:?}", error);

                    *control_flow = ControlFlow::Exit;
                    return;
                }
            }
            Event::MainEventsCleared => {}
            Event::RedrawEventsCleared => {
                app.request_redraw();
            }
            Event::NewEvents(glutin::event::StartCause::Poll) => {
                return;
            }
            Event::DeviceEvent { .. } => (),
            e => println!("{:?}", e),
        }

        for message in order_receiver.try_iter() {
            match message {
                Message::Insert((input_name, input_config)) => {
                    match input_from_config(&app.config.path, &input_config, &input_name) {
                        Ok(input_provider) => {
                            app.uniform_sources.insert(input_name, input_provider);
                        }
                        Err(e) => eprintln!("{:?}", e),
                    }
                }
                Message::Set(set_info) => match set_info {
                    SetInfo::BPM(bpm) => {
                        app.shader_view.set_bpm(bpm);
                    }
                },
            }
        }
    });
}
