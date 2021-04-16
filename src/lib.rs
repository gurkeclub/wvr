use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::thread;

use anyhow::{Context, Result};

use glium::glutin;
use glium::glutin::event::Event;
use glium::glutin::event_loop::ControlFlow;
use glium::glutin::event_loop::EventLoop;
use glutin::event::WindowEvent;

use wvr_com::data::{Message, SetInfo};
use wvr_data::config::project_config::ProjectConfig;
use wvr_data::InputProvider;
use wvr_rendering::RGBAImageData;
use wvr_rendering::ShaderView;

pub mod utils;

pub struct Wvr {
    pub project_path: PathBuf,
    pub config: ProjectConfig,

    pub order_receiver: Receiver<Message>,

    pub uniform_sources: HashMap<String, Box<dyn InputProvider>>,

    pub shader_view: ShaderView,

    focused: bool,
    mouse_position: (f64, f64),

    screenshot_sender: SyncSender<(RGBAImageData, usize)>,
    _screenshot_thread: thread::JoinHandle<()>,
}

impl Wvr {
    pub fn new(
        project_path: &Path,
        config: ProjectConfig,
        event_loop: &EventLoop<()>,
        order_receiver: Receiver<Message>,
    ) -> Result<Self> {
        let available_filter_list = utils::load_available_filter_list(project_path)?;

        let shader_view = ShaderView::new(
            config.bpm as f64,
            &config.view,
            &config.render_chain,
            &config.final_stage,
            &available_filter_list,
            event_loop,
        )?;

        let (screenshot_sender, screenshot_receiver): (
            SyncSender<(RGBAImageData, usize)>,
            Receiver<(RGBAImageData, usize)>,
        ) = sync_channel(60);

        let screenshot_thread = {
            let screenshot_path = config.view.screenshot_path.clone();

            if config.view.screenshot && !screenshot_path.exists() {
                fs::create_dir_all(&screenshot_path).context(format!(
                    "Could not create screenshot output folder {:?}",
                    screenshot_path
                ))?;
            }

            thread::spawn(move || {
                let mut v: Vec<u8> = Vec::new();
                for (image_data, frame_count) in screenshot_receiver.iter() {
                    if image_data.data.len() * 3 != v.len() {
                        v = vec![0; image_data.data.len() * 3];
                    }

                    for (index, (a, b, c, _)) in image_data.data.iter().enumerate() {
                        v[index * 3] = *a;
                        v[index * 3 + 1] = *b;
                        v[index * 3 + 2] = *c;
                    }

                    let image_path = screenshot_path.join(format!("{:012}.bmp", frame_count));

                    image::save_buffer(
                        &image_path,
                        &v,
                        image_data.width,
                        image_data.height,
                        image::ColorType::Rgb8,
                    )
                    .unwrap();
                }
            })
        };
        let mut uniform_sources = HashMap::new();

        for (input_name, input_config) in &config.inputs {
            let input_provider =
                utils::input_from_config(project_path, &input_config, &input_name)?;

            uniform_sources.insert(input_name.clone(), input_provider);
        }

        Ok(Self {
            project_path: project_path.to_owned(),
            config,

            order_receiver,

            uniform_sources,

            shader_view,

            focused: false,
            mouse_position: (0.0, 0.0),

            screenshot_sender,
            _screenshot_thread: screenshot_thread,
        })
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    pub fn set_mouse_position(&mut self, position: (f64, f64)) {
        self.mouse_position = position;
        self.shader_view.set_mouse_position(self.mouse_position);
    }

    pub fn update(&mut self) -> Result<()> {
        self.shader_view.update(&mut self.uniform_sources)?;

        Ok(())
    }

    pub fn render(&mut self) -> Result<()> {
        self.shader_view.render()?;

        if self.config.view.screenshot {
            if let Err(e) = self.screenshot_sender.send((
                self.shader_view.take_screenshot()?,
                self.shader_view.get_frame_count(),
            )) {
                eprintln!(
                    "Screenshot processing thread seems to have crashed:\n {:?}",
                    e
                );
                self.config.view.screenshot = false;
            }
        }

        Ok(())
    }

    pub fn request_redraw(&mut self) {
        self.shader_view.request_redraw();
    }

    pub fn stop(&mut self) {
        for (_input_name, source) in self.uniform_sources.iter_mut() {
            source.stop();
        }
    }
}

pub fn start_wvr(mut wvr: Wvr, event_loop: EventLoop<()>) {
    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::WindowEvent { event, .. } => {
                if let WindowEvent::CloseRequested = event {
                    *control_flow = ControlFlow::Exit;

                    wvr.stop();
                    return;
                } else if let WindowEvent::Focused(focused) = event {
                    wvr.set_focused(focused);
                } else if let WindowEvent::CursorMoved { position, .. } = event {
                    wvr.set_mouse_position((position.x, position.y));
                } else {
                    //println!("{:?}", event);
                }
            }
            Event::RedrawRequested(_) => {
                if let Err(error) = wvr.update() {
                    eprintln!("Failed to update app: {:?}", error);

                    *control_flow = ControlFlow::Exit;
                    return;
                }

                if let Err(error) = wvr.render() {
                    eprintln!("Failed to update app: {:?}", error);

                    *control_flow = ControlFlow::Exit;
                    return;
                }
            }
            Event::MainEventsCleared => {}
            Event::RedrawEventsCleared => {
                wvr.request_redraw();
            }
            Event::NewEvents(glutin::event::StartCause::Poll) => {
                return;
            }
            Event::DeviceEvent { .. } => (),
            e => println!("{:?}", e),
        }

        for message in wvr.order_receiver.try_iter() {
            match message {
                Message::Insert((input_name, input_config)) => {
                    match utils::input_from_config(&wvr.project_path, &input_config, &input_name) {
                        Ok(input_provider) => {
                            wvr.uniform_sources.insert(input_name, input_provider);
                        }
                        Err(e) => eprintln!("{:?}", e),
                    }
                }
                Message::Set(set_info) => match set_info {
                    SetInfo::BPM(bpm) => {
                        wvr.shader_view.set_bpm(bpm);
                    }
                },
            }
        }
    });
}
