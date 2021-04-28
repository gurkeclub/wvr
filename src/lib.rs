use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::thread;

use anyhow::{Context, Result};

use glium::backend::Facade;
use glium::glutin;
use glium::glutin::event::Event;
use glium::glutin::event_loop::ControlFlow;
use glium::glutin::event_loop::EventLoop;
use glium::Display;
use glium::Frame;
use glutin::event::WindowEvent;

use wvr_com::data::{InputUpdate, Message, RenderStageUpdate, SetInfo};
use wvr_data::config::project_config::{ProjectConfig, Speed};
use wvr_data::{DataHolder, InputProvider};
use wvr_rendering::stage::Stage;
use wvr_rendering::RGBAImageData;
use wvr_rendering::ShaderView;

pub mod utils;

pub struct Wvr {
    pub project_path: PathBuf,
    pub config: ProjectConfig,

    pub uniform_sources: HashMap<String, Box<dyn InputProvider>>,

    pub shader_view: ShaderView,

    focused: bool,
    mouse_position: (f64, f64),

    screenshot_sender: SyncSender<(RGBAImageData, usize)>,
    _screenshot_thread: thread::JoinHandle<()>,
}

impl Wvr {
    pub fn new(project_path: &Path, config: ProjectConfig, display: &dyn Facade) -> Result<Self> {
        let mut available_filter_list =
            utils::load_available_filter_list(&wvr_data::get_filters_path(), true)?;
        available_filter_list.extend(utils::load_available_filter_list(
            &project_path.join("filters"),
            false,
        )?);

        let shader_view = ShaderView::new(
            config.bpm as f64,
            &config.view,
            &config.render_chain,
            &config.final_stage,
            &available_filter_list,
            display,
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

        let uniform_sources = utils::load_inputs(project_path, &config.inputs)?;

        Ok(Self {
            project_path: project_path.to_owned(),
            config,

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

    pub fn update(&mut self, display: &dyn Facade, resolution: (usize, usize)) -> Result<()> {
        self.shader_view.set_resolution(display, resolution)?;
        self.shader_view
            .update(display, &mut self.uniform_sources)?;

        Ok(())
    }

    pub fn render_stages(&mut self, display: &dyn Facade) -> Result<()> {
        self.shader_view.render_stages(display)?;

        Ok(())
    }

    pub fn render_final_stage(
        &mut self,
        display: &dyn Facade,
        window_frame: &mut Frame,
    ) -> Result<()> {
        self.shader_view.render_final_stage(display, window_frame)?;

        if self.config.view.screenshot {
            if let Err(e) = self.screenshot_sender.send((
                self.shader_view.take_screenshot(display)?,
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

    pub fn handle_message(&mut self, display: &dyn Facade, message: &Message) -> Result<()> {
        match message {
            Message::Insert((input_name, input_config)) => {
                match utils::input_from_config(&self.project_path, &input_config, &input_name) {
                    Ok(input_provider) => {
                        self.uniform_sources
                            .insert(input_name.clone(), input_provider);
                    }
                    Err(e) => eprintln!("{:?}", e),
                }
            }
            Message::Set(set_info) => match set_info {
                SetInfo::Bpm(bpm) => {
                    self.shader_view.set_bpm(*bpm);
                }
                SetInfo::Width(width) => {
                    let previous_dynamic_resolution = self.shader_view.get_dynamic_resolution();

                    self.config.view.width = *width as i64;
                    self.shader_view.set_dynamic_resolution(true);
                    self.shader_view.set_resolution(
                        display,
                        (*width as usize, self.shader_view.get_resolution().1),
                    )?;

                    self.shader_view
                        .set_dynamic_resolution(previous_dynamic_resolution);
                }
                SetInfo::Height(height) => {
                    let previous_dynamic_resolution = self.shader_view.get_dynamic_resolution();

                    self.config.view.height = *height as i64;
                    self.shader_view.set_dynamic_resolution(true);
                    self.shader_view.set_resolution(
                        display,
                        (self.shader_view.get_resolution().0, *height as usize),
                    )?;

                    self.shader_view
                        .set_dynamic_resolution(previous_dynamic_resolution);
                }
                SetInfo::TargetFps(target_fps) => {
                    self.config.view.target_fps = *target_fps as f32;
                    self.shader_view.set_target_fps(*target_fps);
                }
                SetInfo::DynamicResolution(dynamic_resolution) => {
                    self.config.view.dynamic = *dynamic_resolution;
                    self.shader_view.set_dynamic_resolution(*dynamic_resolution);
                }
                SetInfo::VSync(vsync) => {
                    self.config.view.vsync = *vsync;
                }
                SetInfo::Fullscreen(fullscreen) => {
                    self.config.view.fullscreen = *fullscreen;
                }
                SetInfo::LockedSpeed(locked_speed) => {
                    self.config.view.locked_speed = *locked_speed;
                }
                SetInfo::Screenshot(screenshot) => {
                    self.config.view.screenshot = *screenshot;
                }
            },
            Message::RemoveRenderStage(render_stage_index) => {
                self.shader_view.remove_render_stage(*render_stage_index);
            }
            Message::MoveRenderStage(original_index, target_index) => {
                self.shader_view
                    .move_render_stage(*original_index, *target_index);
            }
            Message::AddRenderStage(render_stage_config) => {
                self.shader_view.add_render_stage(
                    display,
                    Stage::from_config(&render_stage_config.name, display, render_stage_config)
                        .context("Failed to build render stage")?,
                )?;
            }
            Message::UpdateRenderStage(render_stage_index, message) => {
                if let Some(ref mut render_stage) = self
                    .shader_view
                    .get_render_chain()
                    .get_mut(*render_stage_index)
                {
                    match message {
                        RenderStageUpdate::Filter(filter_name) => {
                            render_stage.set_filter(filter_name)
                        }
                        RenderStageUpdate::FilterModeParams(filter_mode_params) => {
                            render_stage.set_filter_mode_params(filter_mode_params)
                        }
                        RenderStageUpdate::Variable(variable_name, variable_value) => {
                            render_stage.set_variable(display, variable_name, variable_value)?;
                        }
                        RenderStageUpdate::VariableAutomation(
                            variable_name,
                            variable_automation,
                        ) => {
                            render_stage
                                .set_variable_automation(variable_name, variable_automation)?;
                        }
                        RenderStageUpdate::Input(input_name, input) => {
                            render_stage.set_input(input_name, input)
                        }
                        RenderStageUpdate::Precision(precision) => {
                            render_stage.set_precision(precision)
                        }
                        RenderStageUpdate::Name(name) => render_stage.set_name(name),
                    }
                }
            }
            Message::UpdateFinalStage(message) => {
                let render_stage = self.shader_view.get_final_stage();
                match message {
                    RenderStageUpdate::Filter(filter_name) => render_stage.set_filter(filter_name),
                    RenderStageUpdate::FilterModeParams(filter_mode_params) => {
                        render_stage.set_filter_mode_params(filter_mode_params)
                    }
                    RenderStageUpdate::Variable(variable_name, variable_value) => {
                        render_stage.set_variable(display, variable_name, variable_value)?;
                    }
                    RenderStageUpdate::VariableAutomation(variable_name, variable_automation) => {
                        render_stage.set_variable_automation(variable_name, variable_automation)?;
                    }
                    RenderStageUpdate::Input(input_name, input) => {
                        render_stage.set_input(input_name, input)
                    }
                    RenderStageUpdate::Precision(precision) => {
                        render_stage.set_precision(precision)
                    }
                    RenderStageUpdate::Name(name) => render_stage.set_name(name),
                }
            }
            Message::AddInput(input_name, input_config) => {
                match utils::input_from_config(&self.project_path, &input_config, &input_name) {
                    Ok(input_provider) => {
                        self.uniform_sources
                            .insert(input_name.clone(), input_provider);
                    }
                    Err(e) => eprintln!("{:?}", e),
                }
            }
            Message::UpdateInput(input_name, input_order) => {
                if let Some(input) = self.uniform_sources.get_mut(input_name) {
                    match input_order {
                        InputUpdate::SetHeight(new_height) => {
                            input.set_property("height", &DataHolder::Int(*new_height as i32))
                        }
                        InputUpdate::SetWidth(new_width) => {
                            input.set_property("width", &DataHolder::Int(*new_width as i32))
                        }
                        InputUpdate::SetPath(new_path) => {
                            input.set_property("path", &DataHolder::String(new_path.clone()))
                        }
                        InputUpdate::SetSpeed(new_speed) => match new_speed {
                            Speed::Fpb(new_speed) => {
                                input.set_property("speed_fpb", &DataHolder::Float(*new_speed))
                            }
                            Speed::Fps(new_speed) => {
                                input.set_property("speed_fps", &DataHolder::Float(*new_speed))
                            }
                        },
                    }
                }
            }
            Message::RenameInput(old_input_name, new_input_name) => {
                if let Some(mut input) = self.uniform_sources.remove(old_input_name) {
                    input.set_name(new_input_name);
                    self.uniform_sources.insert(new_input_name.clone(), input);
                }
            }
            Message::RemoveInput(input_name) => {
                self.uniform_sources.remove(input_name);
            }
        }

        Ok(())
    }

    pub fn stop(&mut self) {
        for (_input_name, source) in self.uniform_sources.iter_mut() {
            source.stop();
        }
    }
}

pub fn start_wvr(
    display: Display,
    mut wvr: Wvr,
    event_loop: EventLoop<()>,
    order_receiver: Receiver<Message>,
) {
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
                }
            }
            Event::RedrawRequested(_) => {
                let new_resolution = display.get_framebuffer_dimensions();
                let new_resolution = (new_resolution.0 as usize, new_resolution.1 as usize);

                if let Err(error) = wvr.update(&display, new_resolution) {
                    eprintln!("Failed to update app: {:?}", error);

                    *control_flow = ControlFlow::Exit;
                    return;
                }

                if let Err(error) = wvr.render_stages(&display) {
                    eprintln!("Failed to render stages: {:?}", error);

                    *control_flow = ControlFlow::Exit;
                }

                let mut window_frame = display.draw();
                if let Err(error) = wvr.render_final_stage(&display, &mut window_frame) {
                    eprintln!("Failed to render to window: {:?}", error);

                    *control_flow = ControlFlow::Exit;
                }

                window_frame
                    .finish()
                    .context("Failed to finalize rendering")
                    .unwrap();

                if control_flow == &ControlFlow::Exit {
                    return;
                }
            }
            Event::MainEventsCleared => {}
            Event::RedrawEventsCleared => {
                display.gl_window().window().request_redraw();
            }
            Event::NewEvents(glutin::event::StartCause::Poll) => {
                return;
            }
            Event::DeviceEvent { .. } => (),
            e => println!("{:?}", e),
        }

        for message in order_receiver.try_iter() {
            wvr.handle_message(&display, &message).unwrap();
        }
    });
}
