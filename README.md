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
cargo install "https://github.com/gurkeclub/wvr.git" --branch main
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