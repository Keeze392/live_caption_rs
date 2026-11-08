[![Language](https://img.shields.io/badge/language-Rust-orange)](#)
[![Platform](https://img.shields.io/badge/platform-Linux-green)](#)

### **About**
This project is Live caption, it will record any audio and feed the Whisper model to print text on front GUI.

### **Features**
- Support AMD GPU
- Support NVIDIA GPU
- Support translate almost any Languages to English except Model with ".en" name in file cannot translate.
- Support OSC to export the output text
- Support OSC send to Vrchat chatbox
- Support VAD model under settings
- Auto Save all configures by close GUI "x" button
- Save History, Default save file is in Documents path.

### **How to use**
You can download by [Release](https://github.com/Keeze392/live_caption_rs/releases/tag/0.2.0). \
If you have Nvidia GPU -> pick cuda \
If you have AMD GPU -> pick vulkan \
If you have none of above, it will pick CPU automationcally

**For model:** \
pick one of any models on list: \
https://huggingface.co/ggerganov/whisper.cpp/tree/main \
Model level from small knowledge to bigger knowledge: tiny -> base -> small -> medium -> Large

Enjoy!
