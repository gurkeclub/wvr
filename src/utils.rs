use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result};
use clap::{App, Arg};
use git2::Repository;

use wvr_cam::cam::CamProvider;
use wvr_data::config::project_config::{FilterConfig, InputConfig, ProjectConfig};
use wvr_data::InputProvider;
use wvr_image::image::PictureProvider;
use wvr_midi::midi::controller::MidiProvider;
use wvr_video::video::VideoProvider;

pub fn init_wvr_data_directory() -> Result<()> {
    let data_path = wvr_data::get_data_path();

    let libs_path = wvr_data::get_libs_path();
    let lib_std_url = "https://github.com/gurkeclub/wvr-glsl-lib-std";
    let lib_std_path = libs_path.join("std");

    let filter_folder_path = wvr_data::get_filters_path();

    let projects_path = libs_path.join("projects");

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
        wvr_shadertoy::create_project_from_shadertoy_url(
            data_path.as_path(),
            shadertoy_url,
            matches.value_of("shadertoy-key").unwrap(),
        )
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

    if let Ok(resource_path) = resource_path.canonicalize() {
        resource_path
    } else {
        resource_path
    }
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
                *speed,
            )?)
        }
        InputConfig::Picture {
            path,
            width,
            height,
        } => {
            let path = get_path_for_resource(&project_path, &path);

            println!("{:?}", path);
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

pub fn load_available_filter_list(
    project_path: &Path,
) -> Result<HashMap<String, (PathBuf, FilterConfig)>> {
    let mut available_filter_list = HashMap::new();

    let project_filter_folder_path = project_path.join("filters");
    let wvr_filter_folder_path = wvr_data::get_filters_path();

    // Load filters from project
    for folder_entry in project_filter_folder_path.read_dir()? {
        let filter_path = folder_entry?.path();
        let filter_config_path = filter_path.join("config.json");
        if !filter_config_path.exists() {
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

        available_filter_list.insert(filter_name, (filter_path, filter_config));
    }

    // Load filters provided by wvr
    for folder_entry in wvr_filter_folder_path.read_dir()? {
        let filter_path = folder_entry?.path();
        let filter_config_path = filter_path.join("config.json");
        if !filter_config_path.exists() {
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
            .entry(filter_name)
            .or_insert((filter_path, filter_config));
    }

    Ok(available_filter_list)
}
