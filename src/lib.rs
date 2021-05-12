use std::thread;
use std::{collections::HashMap, time::Instant};
use std::{fs, str::FromStr};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use std::{
    sync::mpsc::{sync_channel, Receiver, SyncSender},
    time::Duration,
};

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
use wvr_data::config::project_config::{ProjectConfig, SampledInput, Speed};
use wvr_data::{DataHolder, InputProvider};
use wvr_rendering::stage::Stage;
use wvr_rendering::RGBAImageData;
use wvr_rendering::ShaderView;

pub mod utils;

pub struct Wvr {
    pub project_path: PathBuf,

    pub uniform_sources: HashMap<String, Box<dyn InputProvider>>,

    pub shader_view: ShaderView,

    width: usize,
    height: usize,

    fullscreen: bool,
    vsync: bool,

    bpm: f64,
    target_fps: f64,
    locked_speed: bool,

    last_update_time: Instant,

    frame_count: usize,
    pub time: f64,
    pub beat: f64,

    stopped: bool,
    playing: bool,

    focused: bool,
    mouse_position: (f64, f64),

    screenshot: bool,
    screenshot_sender: SyncSender<(RGBAImageData, usize)>,
    _screenshot_thread: Option<thread::JoinHandle<()>>,
    screenshot_stop: Arc<AtomicBool>,
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
            &config.view,
            &config.render_chain,
            &config.final_stage,
            &available_filter_list,
            display,
        )?;

        let (screenshot_sender, screenshot_receiver): (
            SyncSender<(RGBAImageData, usize)>,
            Receiver<(RGBAImageData, usize)>,
        ) = sync_channel(1);

        let screenshot_stop = Arc::new(AtomicBool::new(false));
        let screenshot_thread = if config.view.screenshot {
            let screenshot_stop = screenshot_stop.clone();
            let screenshot_path = config.view.screenshot_path.clone();
            let screenshot_path = PathBuf::from_str(&utils::get_path_for_resource(
                project_path,
                screenshot_path.to_str().unwrap(),
            ))
            .unwrap();

            if config.view.screenshot && !screenshot_path.exists() {
                fs::create_dir_all(&screenshot_path).context(format!(
                    "Could not create screenshot output folder {:?}",
                    screenshot_path
                ))?;
            }

            let output_path = screenshot_path
                .join("output.mp4")
                .to_str()
                .unwrap()
                .to_owned();

            let view_config = config.view.clone();
            Some(thread::spawn(move || {
                let mut encoder = wvr_video::encoder::VideoEncoder::new(
                    &output_path,
                    view_config.width as usize,
                    view_config.height as usize,
                    view_config.target_fps as f64,
                )
                .unwrap();

                let pixel_count = (view_config.width * view_config.height) as usize;
                let mut raw_frame: Vec<u8> = vec![0; pixel_count * 3];
                loop {
                    if let Ok((image_data, frame_count)) = screenshot_receiver.try_recv() {
                        if image_data.data.len() * 3 != raw_frame.len() {
                            panic!("resolution changed");
                        }

                        for (index, (r, g, b, _)) in image_data.data.into_iter().enumerate() {
                            raw_frame[index * 3] = r;
                            raw_frame[index * 3 + 1] = g;
                            raw_frame[index * 3 + 2] = b;
                        }
                        encoder.encode_frame(
                            frame_count as f64 * view_config.target_fps as f64,
                            &raw_frame,
                        );
                    } else if screenshot_stop.load(Ordering::Relaxed) {
                        break;
                    } else {
                        thread::sleep(Duration::from_millis(1));
                    }
                }
            }))
        } else {
            None
        };

        let uniform_sources = utils::load_inputs(project_path, &config.inputs)?;

        Ok(Self {
            project_path: project_path.to_owned(),

            uniform_sources,

            shader_view,

            width: config.view.width as usize,
            height: config.view.height as usize,

            vsync: config.view.vsync,
            fullscreen: config.view.fullscreen,

            stopped: false,
            playing: false,

            bpm: config.bpm as f64,
            target_fps: config.view.target_fps as f64,
            locked_speed: config.view.locked_speed,

            last_update_time: Instant::now(),

            frame_count: 0,
            time: 0.0,
            beat: 0.0,

            focused: false,
            mouse_position: (0.0, 0.0),

            screenshot: config.view.screenshot,
            screenshot_sender,
            _screenshot_thread: screenshot_thread,

            screenshot_stop,
        })
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    pub fn set_mouse_position(&mut self, position: (f64, f64)) {
        self.mouse_position = position;
        self.shader_view.set_mouse_position(self.mouse_position);
    }

    fn update_time(&mut self, time_diff: f64, beat_diff: f64) {
        self.time += time_diff;
        self.beat += beat_diff;

        for (_, source) in self.uniform_sources.iter_mut() {
            source.set_beat(self.beat, self.locked_speed);
            source.set_time(self.time, self.locked_speed);
        }
    }

    pub fn update(&mut self, display: &dyn Facade, resolution: (usize, usize)) -> Result<()> {
        if self.screenshot {
            self.locked_speed = true;
        }
        let new_update_time = Instant::now();

        let beat_diff = if self.locked_speed {
            self.bpm / (60.0 * self.target_fps)
        } else {
            let time_diff = new_update_time - self.last_update_time;
            time_diff.as_secs_f64() * self.bpm / 60.0
        };

        let time_diff = if self.locked_speed {
            1.0 / self.target_fps
        } else {
            (new_update_time - self.last_update_time).as_secs_f64()
        };

        self.update_time(time_diff, beat_diff);

        if !self.screenshot {
            self.shader_view.set_resolution(display, resolution)?;
        }
        self.shader_view.update(
            display,
            &mut self.uniform_sources,
            self.time,
            self.beat,
            self.frame_count,
        )?;

        self.last_update_time = new_update_time;

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

        if self.screenshot {
            let mut currently_rendered_stage = None;
            if let Some(final_stage_input) = self
                .shader_view
                .get_final_stage()
                .get_input_map()
                .get("iChannel0")
            {
                match final_stage_input {
                    SampledInput::Nearest(input_name) => {
                        currently_rendered_stage = Some(input_name.to_string())
                    }
                    SampledInput::Linear(input_name) => {
                        currently_rendered_stage = Some(input_name.to_string())
                    }
                    SampledInput::Mipmaps(input_name) => {
                        currently_rendered_stage = Some(input_name.to_string())
                    }
                }
            }
            if let Some(currently_rendered_stage) = currently_rendered_stage {
                if let Some(texture) = self.shader_view.take_screenshot(&currently_rendered_stage) {
                    if let Err(e) = self.screenshot_sender.send((texture?, self.frame_count)) {
                        eprintln!(
                            "Screenshot processing thread seems to have crashed:\n {:?}",
                            e
                        );
                        self.screenshot = false;
                    }
                }
            }
        }

        self.frame_count += 1;

        Ok(())
    }

    pub fn handle_message(&mut self, display: &dyn Facade, message: &Message) -> Result<()> {
        match message {
            Message::Start => {
                self.play()?;
            }
            Message::Pause => {
                self.pause()?;
            }
            Message::Stop => {
                self.stop();
            }
            Message::Insert((input_name, input_config)) => {
                match utils::input_from_config(
                    &self.project_path,
                    &input_config,
                    &input_name,
                    self.beat,
                    self.time,
                    self.playing,
                ) {
                    Ok(input_provider) => {
                        self.uniform_sources
                            .insert(input_name.clone(), input_provider);
                    }
                    Err(e) => eprintln!("{:?}", e),
                }
            }
            Message::Set(set_info) => match set_info {
                SetInfo::Bpm(bpm) => {
                    self.bpm = *bpm;
                }
                SetInfo::Width(width) => {
                    let previous_dynamic_resolution = self.shader_view.get_dynamic_resolution();

                    self.width = *width;
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

                    self.height = *height;
                    self.shader_view.set_dynamic_resolution(true);
                    self.shader_view.set_resolution(
                        display,
                        (self.shader_view.get_resolution().0, *height as usize),
                    )?;

                    self.shader_view
                        .set_dynamic_resolution(previous_dynamic_resolution);
                }
                SetInfo::TargetFps(target_fps) => {
                    self.target_fps = *target_fps;
                }
                SetInfo::DynamicResolution(dynamic_resolution) => {
                    self.shader_view.set_dynamic_resolution(*dynamic_resolution);
                }
                SetInfo::VSync(vsync) => {
                    self.vsync = *vsync;
                }
                SetInfo::Fullscreen(fullscreen) => {
                    self.fullscreen = *fullscreen;
                }
                SetInfo::LockedSpeed(locked_speed) => {
                    self.locked_speed = *locked_speed;
                }
                SetInfo::Screenshot(screenshot) => {
                    self.screenshot = *screenshot;
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
                match utils::input_from_config(
                    &self.project_path,
                    &input_config,
                    &input_name,
                    self.beat,
                    self.time,
                    self.playing,
                ) {
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

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn stop(&mut self) {
        if self.stopped {
            return;
        }

        for (_input_name, source) in self.uniform_sources.iter_mut() {
            if let Err(e) = source.stop() {
                eprintln!("{:?}", e);
            }
        }

        self.screenshot_stop.store(true, Ordering::Relaxed);

        self.stopped = true;
        self.playing = false;
    }

    pub fn pause(&mut self) -> Result<()> {
        if self.stopped {
            return Ok(());
        }

        for (_input_name, source) in self.uniform_sources.iter_mut() {
            source.pause()?;
        }

        self.playing = false;

        Ok(())
    }

    pub fn play(&mut self) -> Result<()> {
        if self.stopped || self.playing {
            return Ok(());
        }

        self.last_update_time = Instant::now();
        self.update_time(0.0, 0.0);

        for (_input_name, source) in self.uniform_sources.iter_mut() {
            source.play()?;
        }

        self.playing = true;

        Ok(())
    }

    pub fn get_width(&self) -> usize {
        self.width
    }

    pub fn get_height(&self) -> usize {
        self.height
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

                if wvr.is_playing() {
                    if let Err(error) = wvr.update(&display, new_resolution) {
                        eprintln!("Failed to update app: {:?}", error);

                        *control_flow = ControlFlow::Exit;
                        return;
                    }

                    if let Err(error) = wvr.render_stages(&display) {
                        eprintln!("Failed to render stages: {:?}", error);

                        *control_flow = ControlFlow::Exit;
                    }
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
