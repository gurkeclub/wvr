# Wvr
Wvr is a shader-based image processing framework being developped at gurke.club for artistic animation production and vjing performances.

The name "wvr" is an abbreviation of the word weaver and has been chosen as this frameworks can be seen as a pixel weaving system.


## Features
 - Configuration files
 - Multistage rendering
 - GLSL shaders hot-reloading
 - Video file support
 - Image file support
 - Camera support
 - Midi controller

 - Remote control 
 - Shadertoy import 

## Platform support
Wvr has been succesfully built and run on the following plateforms:
 - Linux: 
   - Arch Linux x86_64
 - Windows: 
   - Windows 10 x86_64 with mingw-w64

## Dependencies
 - gstreamer-1.0

## Usage
Starting an animation installed in the default project folder:
```
wvr example_simple 
```

Starting an animation from it's configuration file:
```
wvr -c example_simple/config.ron
```

Importing and starting an animation from shadertoy:
```
wvr -s "https://www.shadertoy.com/view/xxxxxx" -k SHADERTOY_API_KEY
```

## Building from scratch

### 1. Installing the gstreamer development libraries
See https://gstreamer.freedesktop.org/documentation/installing/


### 2. Installing wvr
```
cargo install --git "https://github.com/gurkeclub/wvr.git" --branch main
```

## Using prebuilt binaires

### 1. Installing the gstreamer libraries
See https://gstreamer.freedesktop.org/documentation/installing/

### 2. Download the wvr binary
Download the [latest release](https://github.com/gurkeclub/wvr/releases/) and place it in a folder available through your PATH environment variable

## Wvr related libraries
 - [wvr-data](https://github.com/gurkeclub/wvr-data)
 - [wvr-rendering](https://github.com/gurkeclub/wvr-rendering)
 - [wvr-video](https://github.com/gurkeclub/wvr-video)
 - [wvr-image](https://github.com/gurkeclub/wvr-image)
 - [wvr-cam](https://github.com/gurkeclub/wvr-cam)
 - [wvr-midi](https://github.com/gurkeclub/wvr-midi)
 - [wvr-shadertoy](https://github.com/gurkeclub/wvr-shadertoy)
 - [wvr-com](https://github.com/gurkeclub/wvr-com)


## Animation configuration
### Example of a configuration for an animation
The following code is a copy of the [simple example](https://github.com/gurkeclub/wvr-examples/blob/main/simple/config.ron) animation for wvr:


```json
(
    view: (
        bpm: 89,
        width: 640,
        height: 480,
        target_fps: 60,
        dynamic: true,
        fullscreen: false,
        vsync: false,
        screenshot: false,
        screenshot_path: "output/",
        locked_speed: false,
    ),
    server: (
        ip: "127.0.0.1",
        port: 3000,
        enable: false,
    ),
    inputs: {
        "forest": (
            type: "Picture",
            path: "res/forest.jpg",
            width: 640,
            height: 480,
        ),
        "butterfly": (
            type: "Video",
            path: "res/butterfly.mp4",
            width: 640,
            height: 480,
            speed: (Fps: 30.0),
        ),
    },
    filters: {
        "target": (
            inputs: [
                "iChannel0",
            ],
            vertex_shader: [
                "#std/default.vs.glsl",
            ],
            fragment_shader: [
                "#std/header.glsl",
                "render_chain/target.fs.glsl",
            ],
            variables: {},
        ),
        "collage": (
            inputs: [
                "iChannel0",
                "iChannel1",
            ],
            vertex_shader: [
                "#std/default.vs.glsl",
            ],
            fragment_shader: [
                "#std/header.glsl",
                "render_chain/collage.fs.glsl",
            ],
            variables: {},
        ),
    },
    render_chain: [
        (
            name: "collage",
            filter: "collage",
            inputs: {
                "iChannel0": Linear("forest"),
                "iChannel1": Linear("butterfly"),
            },
            variables: {},
            precision: U8,
        ),
    ],
    final_stage: (
        name: "target",
        filter: "target",
        inputs: {
            "iChannel0": Linear("collage"),
        },
        variables: {},
        precision: U8,
    ),
)

```
