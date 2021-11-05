use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result};
use clap::{App, Arg};
use git2::Repository;

use glium::glutin;
use glium::glutin::event_loop::EventLoop;
use glium::Display;

use glutin::dpi::PhysicalSize;
use glutin::window::WindowBuilder;
use glutin::ContextBuilder;

use wvr_cam::cam::CamProvider;
use wvr_data::config::filter::FilterConfig;
use wvr_data::config::input::InputConfig;
use wvr_data::config::project::ProjectConfig;
use wvr_data::config::project::ViewConfig;
use wvr_data::types::InputProvider;
use wvr_image::image::PictureProvider;
use wvr_midi::midi::controller::MidiProvider;
use wvr_video::video::VideoProvider;

pub fn init_wvr_data_directory() -> Result<()> {
    let data_path = wvr_data::get_data_path();

    let libs_path = wvr_data::get_libs_path();
    let lib_std_url = "https://github.com/gurkeclub/wvr-glsl-lib-std";
    let lib_std_path = libs_path.join("std");

    let filter_folder_path = wvr_data::get_filters_path();

    let projects_path = data_path.join("projects");

    if !data_path.exists() {
        println!("Creating data directory at {:?}", &data_path);
        fs::create_dir_all(&data_path).context("Failed to create data directory")?;
    }

    if !libs_path.exists() {
        println!("Creating glsl libs directory at {:?}", &libs_path);
        fs::create_dir_all(&libs_path).unwrap();
    }

    if !lib_std_path.exists() {
        println!("\tDownloading glsl standard library to {:?}", lib_std_path);
        Repository::clone(lib_std_url, lib_std_path)
            .context("Failed to init glsl standard library")?;
    }

    if !filter_folder_path.exists() {
        println!("Creating filters directory at {:?}", &filter_folder_path);
        fs::create_dir_all(&filter_folder_path).context("Failed to create filters directory")?;
    }

    if !projects_path.exists() {
        println!("Creating projects_path directory at {:?}", &projects_path);
        fs::create_dir_all(&projects_path).context("Failed to create filters directory")?;
    }

    Ok(())
}

pub fn get_config() -> Result<(PathBuf, ProjectConfig)> {
    let data_path = wvr_data::get_data_path();

    let matches = App::new("Wvr")
        .version("0.0.1")
        .author("Gurke.Club <contact@gurke.club>")
        .about("A VJ-focused image processing framework")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .about("Allows loading a project outside of the default project path")
                .required(false)
                .takes_value(true),
        )
        .arg(
            Arg::new("project_name")
                .about("Sets the input file to use")
                .required(false)
                .index(1),
        )
        .arg(
            Arg::new("shadertoy")
                .short('s')
                .long("shadertoy")
                .value_name("URL")
                .about("Allows import of a shadertoy based project")
                .required(false)
                .takes_value(true),
        )
        .arg(
            Arg::new("shadertoy-key")
                .short('k')
                .long("shadertoy-key")
                .value_name("KEY")
                .about("Provides the api key for shadertoy import")
                .required(false)
                .takes_value(true),
        )
        .arg(
            Arg::new("new")
                .short('n')
                .long("new")
                .value_name("NAME")
                .about("Allows creation of a default project")
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
            data_path
                .join("projects")
                .join(project_name)
                .join("config.json"),
        )
    } else if let Some(shadertoy_url) = matches.value_of("shadertoy") {
        Some(wvr_shadertoy::create_project_from_shadertoy_url(
            data_path.as_path(),
            shadertoy_url,
            matches.value_of("shadertoy-key").unwrap(),
        )?)
    } else {
        None
    };

    let config_path = config_path.unwrap();

    let project_path = config_path.parent().unwrap().to_owned();
    let config: ProjectConfig = if let Ok(file) = File::open(&config_path) {
        serde_json::from_reader::<File, ProjectConfig>(file).unwrap()
    } else {
        panic!("Could not find config file {:?}", config_path);
    };

    Ok((project_path, config))
}

pub fn get_path_for_resource<P: AsRef<Path>>(path: P, resource_path: &str) -> String {
    if resource_path.starts_with("http") {
        return resource_path.to_owned();
    }

    let resource_path = resource_path.replace('\\', "/");

    if let Ok(abs_resource_path) = fs::canonicalize(&PathBuf::from(&resource_path)) {
        if abs_resource_path.to_str().unwrap() == resource_path {
            return resource_path;
        }
    }

    let resource_path = path.as_ref().join(resource_path);

    let resource_path = PathBuf::from(
        &(&resource_path)
            .to_str()
            .unwrap()
            .to_string()
            .replace('\\', "/"),
    );

    return resource_path.as_path().to_str().unwrap().to_string();

    /*
    if let Ok(resource_path) = resource_path.canonicalize() {
        resource_path
    } else {
        resource_path
    }
    .as_path()
    .to_str()
    .unwrap()
    .to_string()
     */
}

pub fn input_from_config<P: AsRef<Path>>(
    project_path: P,
    input_config: &InputConfig,
    input_name: &str,
    current_beat: f64,
    current_time: f64,
    wvr_playing: bool,
) -> Result<Box<dyn InputProvider>> {
    let input: Box<dyn InputProvider> = match input_config {
        InputConfig::Video {
            path,
            width,
            height,
            speed,
        } => {
            let path = get_path_for_resource(&project_path, path);
            Box::new(VideoProvider::new(
                &path,
                input_name.to_owned(),
                (*width, *height),
                *speed,
                current_beat,
                current_time,
                wvr_playing,
            )?)
        }
        InputConfig::Picture {
            path,
            width,
            height,
        } => {
            let path = get_path_for_resource(&project_path, path);

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
            let path = get_path_for_resource(&project_path, path);
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

pub fn load_available_filter_list(
    searched_path: &Path,
    is_system_filter: bool,
) -> Result<HashMap<String, (PathBuf, FilterConfig, bool)>> {
    let mut available_filter_list = HashMap::new();

    if searched_path.exists() && searched_path.is_dir() {
        for folder_entry in searched_path.read_dir()? {
            let filter_path = folder_entry?.path();
            let filter_config_path = filter_path.join("config.json");

            if !filter_config_path.exists() {
                let prefix = filter_path
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned();

                let subfolder_filters = load_available_filter_list(&filter_path, is_system_filter)?;

                for (filter_name, filter_info) in subfolder_filters {
                    available_filter_list
                        .insert(format!("{:}/{:}", prefix, filter_name), filter_info);
                }

                continue;
            }

            let filter_name = filter_path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();

            let filter_config: FilterConfig =
                serde_json::from_reader::<File, FilterConfig>(File::open(&filter_config_path)?)
                    .unwrap();

            available_filter_list
                .insert(filter_name, (filter_path, filter_config, is_system_filter));
        }
    }

    Ok(available_filter_list)
}

pub fn load_inputs(
    project_path: &Path,
    input_list: &HashMap<String, InputConfig>,
) -> Result<HashMap<String, Box<dyn InputProvider>>> {
    let mut uniform_sources = HashMap::new();

    for (input_name, input_config) in input_list {
        let input_provider =
            input_from_config(project_path, input_config, input_name, 0.0, 0.0, true)?;

        uniform_sources.insert(input_name.clone(), input_provider);
    }

    Ok(uniform_sources)
}

pub fn build_window(view_config: &ViewConfig, events_loop: &EventLoop<()>) -> Result<Display> {
    let context = ContextBuilder::new()
        .with_vsync(view_config.vsync)
        .with_srgb(true);
    let fullscreen = if view_config.fullscreen {
        let monitor = events_loop.primary_monitor();
        if let Some(monitor) = monitor {
            let mut selected_monitor = Some(glium::glutin::window::Fullscreen::Exclusive(
                monitor.video_modes().next().unwrap(),
            ));
            for secondary_monitor in events_loop.available_monitors() {
                if secondary_monitor != monitor {
                    selected_monitor = Some(glium::glutin::window::Fullscreen::Exclusive(
                        secondary_monitor.video_modes().next().unwrap(),
                    ));
                }
            }
            selected_monitor
        } else {
            None
        }
    } else {
        None
    };

    let window = WindowBuilder::new()
        .with_inner_size(PhysicalSize::new(
            view_config.width as u32,
            view_config.height as u32,
        ))
        .with_resizable(view_config.dynamic)
        .with_fullscreen(fullscreen)
        .with_title("wvr");

    let window = if view_config.dynamic {
        window
    } else {
        window
            .with_min_inner_size(PhysicalSize::new(
                view_config.width as u32,
                view_config.height as u32,
            ))
            .with_max_inner_size(PhysicalSize::new(
                view_config.width as u32,
                view_config.height as u32,
            ))
    };

    Display::new(window, context, events_loop).context("Failed to create the rendering window")
}
