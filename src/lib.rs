use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::thread;

use anyhow::{Context, Result};
use clap::{App, Arg};

use glium::glutin::event_loop::EventLoop;

use wvr_cam::cam::CamProvider;
use wvr_data::config::project_config::{InputConfig, ProjectConfig};
use wvr_image::image::PictureProvider;
use wvr_data::InputProvider;
use wvr_midi::midi::controller::MidiProvider;
use wvr_rendering::RGBAImageData;
use wvr_rendering::ShaderView;
use wvr_video::video::VideoProvider;

pub fn get_path_for_resource<P: AsRef<Path>>(path: P, resource_path: &str) -> String {
    if resource_path.starts_with("http") {
        return resource_path.to_owned();
    }

    if let Ok(abs_resource_path) = fs::canonicalize(&PathBuf::from(resource_path)) {
        if abs_resource_path.to_str().unwrap() == resource_path {
            return resource_path.to_owned();
        }
    }

    path.as_ref()
        .join(resource_path)
        .as_path()
        .to_str()
        .unwrap()
        .to_string()
}

pub fn input_from_config<P: AsRef<Path>>(
    project_path: P,
    input_config: &InputConfig,
    input_name: &str,
) -> Result<Box<dyn InputProvider>> {
    let input: Box<dyn InputProvider> = match input_config {
        InputConfig::Video {
            path,
            width,
            height,
            speed,
        } => {
            let path = get_path_for_resource(&project_path, &path);
            Box::new(VideoProvider::new(
                &path,
                input_name.to_owned(),
                (*width, *height),
                speed.clone(),
            )?)
        }
        InputConfig::Picture {
            path,
            width,
            height,
        } => {
            let path = get_path_for_resource(&project_path, &path);
            Box::new(PictureProvider::new(
                &path,
                input_name.to_owned(),
                (*width, *height),
            )?)
        }
        InputConfig::Cam {
            path,
            width,
            height,
        } => {
            let path = get_path_for_resource(&project_path, &path);
            Box::new(CamProvider::new(
                &path,
                input_name.to_owned(),
                (*width as usize, *height as usize),
            )?)
        }
        InputConfig::Midi { name } => {
            Box::new(MidiProvider::new(input_name.to_owned(), name.clone())?)
        }
    };

    Ok(input)
}

pub struct VBoij {
    pub config: ProjectConfig,
    pub uniform_sources: HashMap<String, Box<dyn InputProvider>>,
    pub shader_view: ShaderView,

    focused: bool,
    mouse_position: (f64, f64),

    screenshot_sender: SyncSender<(RGBAImageData, usize)>,
    _screenshot_thread: thread::JoinHandle<()>,
}

impl VBoij {
    pub fn new(config: ProjectConfig, event_loop: &EventLoop<()>) -> Result<Self> {
        let shader_view = ShaderView::new(
            &config,
            &config.view,
            &config.render_chain,
            &config.final_stage,
            &config.filters,
            &event_loop,
        )?;

        let (screenshot_sender, screenshot_receiver): (
            SyncSender<(RGBAImageData, usize)>,
            Receiver<(RGBAImageData, usize)>,
        ) = sync_channel(60);

        let screenshot_thread = {
            let width = config.view.width;
            let height = config.view.height;
            let screenshot_path = config.view.screenshot_path.clone();
            if !screenshot_path.exists() {
                fs::create_dir_all(&screenshot_path).context(format!(
                    "Could not create screenshot output folder {:?}",
                    screenshot_path
                ))?;
            }

            thread::spawn(move || {
                let mut v: Vec<u8> = vec![0; width as usize * height as usize * 3];
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
            let input_provider = input_from_config(&config.path, &input_config, &input_name)?;

            uniform_sources.insert(input_name.clone(), input_provider);
        }

        Ok(Self {
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

    pub fn update(&mut self) -> Result<()> {
        self.shader_view
            .update(&self.config, &mut self.uniform_sources)?;

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
}

pub fn get_config() -> Result<ProjectConfig> {
    let wvr_data_path = wvr_data::get_data_path();
    let libs_path = wvr_data::get_libs_path();
    let filters_path = wvr_data::get_filters_path();

    if !wvr_data_path.exists() {
        println!(
            "Warning: The default path for the data directory which contains wvr's projects does not exist, creating it at {:?}",
            &wvr_data_path
        );
        fs::create_dir_all(&wvr_data_path).unwrap();
    }

    if !libs_path.exists() {
        println!(
            "Warning: The default path for the glsl libraries does not exist, creating it at {:?}",
            libs_path.to_str()
        );
        fs::create_dir_all(&libs_path).unwrap();
    }

    if !filters_path.exists() {
        println!(
            "Warning: The default path for the filters folder does not exist, creating it at {:?}",
            filters_path.to_str()
        );
        fs::create_dir_all(&filters_path).unwrap();
    }

    let matches = App::new("VBoij")
        .version("0.0.1")
        .author("Gurke.Club <contact@gurke.club>")
        .about("A VJ-focused image processing framework")
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("FILE")
                .help("Allows loading a project outside of the default project path")
                .required(false)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("project_name")
                .help("Sets the input file to use")
                .required(false)
                .index(1),
        )
        .arg(
            Arg::with_name("shadertoy")
                .short("s")
                .long("shadertoy")
                .value_name("URL")
                .help("Allows import of a shadertoy based project")
                .required(false)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("shadertoy-key")
                .short("k")
                .long("shadertoy-key")
                .value_name("KEY")
                .help("Provides the api key for shadertoy import")
                .required(false)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("new")
                .short("n")
                .long("new")
                .value_name("NAME")
                .help("Allows creation of a default project")
                .required(false)
                .takes_value(true),
        )
        .get_matches();

    let config_path = if let Some(config_path) = matches.value_of("config") {
        let mut config_path = PathBuf::from_str(config_path).unwrap();
        config_path = fs::canonicalize(&config_path).unwrap();
        Some(config_path)
    } else if let Some(project_name) = matches.value_of("project_name") {
        Some(
            wvr_data_path
                .join("projects")
                .join(project_name)
                .join("config.ron"),
        )
    } else if let Some(shadertoy_url) = matches.value_of("shadertoy") {
        wvr_shadertoy::create_project_from_shadertoy_url(
            wvr_data_path.as_path(),
            shadertoy_url,
            matches.value_of("shadertoy-key").unwrap(),
        )
    } else {
        None
    };

    let config_path = config_path.unwrap();

    let project_path = config_path.parent().unwrap().to_owned();
    let mut config: ProjectConfig = if let Ok(file) = File::open(&config_path) {
        ron::de::from_reader::<File, ProjectConfig>(file).unwrap()
    } else {
        panic!("Could not find config file {:?}", project_path);
    };

    config.path = project_path;

    if config.filters_path.to_string_lossy().len() == 0 {
        config.filters_path = filters_path;
    }
    if config.libs_path.to_string_lossy().len() == 0 {
        config.libs_path = libs_path;
    }

    Ok(config)
}