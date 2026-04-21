---These are global variables given to us by the Resolve embedded LuaJIT environment
---I disable the undefined global warnings for them to stop my editor from complaining
---@diagnostic disable: undefined-global
local ffi = ffi

local DEV_MODE = rawget(_G, "AUTOSUBS_DEV_MODE") == true

local function join_path(dir, filename)
    local sep = package.config:sub(1,1) -- returns '\\' on Windows, '/' elsewhere
    if dir:sub(-1) == sep then
        return dir .. filename
    else
        return dir .. sep .. filename
    end
end

-- Simple logger: prints to Resolve console and appends to a log file in TEMP.
local log_path = nil
local function log(msg)
    local ts = os.date("%Y-%m-%d %H:%M:%S")
    local line = "[" .. ts .. "] " .. tostring(msg)
    print(line)
    if log_path then
        local f = io.open(log_path, "a")
        if f then
            f:write(line .. "\n")
            f:close()
        end
    end
end

-- Detect the operating system via ffi.os (no ffi.cdef needed)
local os_name = ffi.os
log("MarksAmazingSubs.lua starting on OS: " .. os_name)

local resources_folder = nil
local app_executable = nil

if os_name == "Windows" then
    -- Set log path
    local temp_dir = os.getenv("TEMP") or os.getenv("TMP") or "C:\\Temp"
    log_path = temp_dir .. "\\MarksAmazingSubs_launch.log"
    log("Log file: " .. log_path)

    -- Read install_path.txt using plain io.open (path is always ASCII)
    local storage_path = os.getenv("APPDATA") ..
        "\\Blackmagic Design\\DaVinci Resolve\\Support\\Fusion\\Scripts\\Utility\\MarksAmazingSubs"
    log("Reading install_path.txt from: " .. storage_path)

    local install_path
    local f = io.open(storage_path .. "\\install_path.txt", "r")
    if f then
        install_path = f:read("*a"):match("^%s*(.-)%s*$")
        f:close()
        log("Install path from install_path.txt: " .. install_path)
    else
        local fallback = os.getenv("LOCALAPPDATA") .. "\\Marks Amazing Subtitles"
        log("install_path.txt not found, trying fallback: " .. fallback)
        local exe_check = io.open(fallback .. "\\autosubs.exe", "rb")
        if exe_check then
            exe_check:close()
            install_path = fallback
            log("Fallback install path found OK")
        else
            local err = "Could not find Marks Amazing Subtitles.\n" ..
                "Tried fallback: " .. fallback .. "\\autosubs.exe\n" ..
                "Install the app and restart DaVinci Resolve."
            log("ERROR: " .. err)
            error(err)
        end
    end

    app_executable = install_path .. "\\autosubs.exe"
    resources_folder = install_path .. "\\resources"

    log("App executable: " .. app_executable)
    log("Resources folder: " .. resources_folder)

    local exe_check = io.open(app_executable, "rb")
    if exe_check then
        exe_check:close()
        log("Executable found OK")
    else
        local err = "App executable not found at: " .. app_executable ..
            "\nThe app may not be installed correctly. Try reinstalling from the setup .exe."
        log("ERROR: " .. err)
        error(err)
    end

elseif os_name == "OSX" then
    app_executable = "/Applications/Marks Amazing Subtitles.app"
    resources_folder = app_executable .. "/Contents/Resources/resources"
else
    app_executable = "/usr/bin/marks-amazing-subtitles"
    resources_folder = "/usr/lib/marks-amazing-subtitles/resources"
end

if DEV_MODE then
    resources_folder = os.getenv("HOME") .. "/Documents/auto-subs/AutoSubs-App/src-tauri/resources"
end

log("Loading autosubs_core module from: " .. resources_folder)

local modules_path = join_path(resources_folder, "modules")
package.path = package.path .. ";" .. join_path(modules_path, "?.lua")

log("Calling App:Init...")
local ok, err = pcall(function()
    local App = require("autosubs_core")
    App:Init(app_executable, resources_folder, DEV_MODE)
end)

if not ok then
    log("ERROR launching app: " .. tostring(err))
    error(err)
end

log("App:Init returned successfully")

_G.AUTOSUBS_DEV_MODE = nil
