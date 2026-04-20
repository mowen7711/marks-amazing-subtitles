---These are global variables given to us by the Resolve embedded LuaJIT environment
---I disable the undefined global warnings for them to stop my editor from complaining
---@diagnostic disable: undefined-global
local ffi = ffi

local DEV_MODE = rawget(_G, "AUTOSUBS_DEV_MODE") == true

local function join_path(dir, filename)
    local sep = package.config:sub(1,1) -- returns '\\' on Windows, '/' elsewhere
    -- Remove trailing separator from dir, if any
    if dir:sub(-1) == sep then
        return dir .. filename
    else
        return dir .. sep .. filename
    end
end

-- Simple logger: prints to Resolve console and appends to a log file in TEMP.
-- Useful for diagnosing startup issues when the app itself cannot run.
local log_path = nil
local function log(msg)
    local ts = os.date("%Y-%m-%d %H:%M:%S")
    local line = "[" .. ts .. "] " .. tostring(msg)
    print(line)
    -- Write to a log file alongside other temp files so it survives app crashes
    if log_path then
        local f = io.open(log_path, "a")
        if f then
            f:write(line .. "\n")
            f:close()
        end
    end
end

-- Helper to convert a UTF-8 string to a wide-character (WCHAR) string
local function to_wide_string(str)
    local len = #str + 1 -- Include null terminator
    local buffer = ffi.new("WCHAR[?]", len)
    local bytes_written = ffi.C.MultiByteToWideChar(65001, 0, str, -1, buffer, len)
    if bytes_written == 0 then
        error("Failed to convert string to wide string: " .. str)
    end
    return buffer
end

-- Function to read the content of a file using _wfopen
local function read_file(file_path)
    if (ffi.os == "Windows") then
        local wide_path = to_wide_string(file_path)
        local mode = to_wide_string("rb")
        local f = ffi.C._wfopen(wide_path, mode)
        if f == nil then
            error("Failed to open file: " .. file_path)
        end

        local buffer = {}
        local temp_buffer = ffi.new("char[4096]") -- 4KB buffer for reading
        while true do
            local read_bytes = ffi.C.fread(temp_buffer, 1, 4096, f)
            if read_bytes == 0 then
                break
            end
            buffer[#buffer + 1] = ffi.string(temp_buffer, read_bytes)
        end
        ffi.C.fclose(f)

        return table.concat(buffer)
    else
        local file = assert(io.open(file_path, "r")) -- Open file for reading
        local content = file:read("*a")              -- Read the entire file content
        file:close()
        return content
    end
end

-- Detect the operating system
local os_name = ffi.os
log("MarksAmazingSubs.lua starting on OS: " .. os_name)

-- Path to the script to launch
local resources_folder = nil
local app_executable = nil

if os_name == "Windows" then
    -- Define the necessary Windows API functions using FFI to prevent special character issues
    ffi.cdef [[
        typedef wchar_t WCHAR;

        int MultiByteToWideChar(
            unsigned int CodePage,
            unsigned long dwFlags,
            const char* lpMultiByteStr,
            int cbMultiByte,
            WCHAR* lpWideCharStr,
            int cchWideChar);

        void* _wfopen(const WCHAR* filename, const WCHAR* mode);
        size_t fread(void* buffer, size_t size, size_t count, void* stream);
        int fclose(void* stream);
    ]]

    -- Set log path now that we know we're on Windows
    local temp_dir = os.getenv("TEMP") or os.getenv("TMP") or "C:\\Temp"
    log_path = temp_dir .. "\\MarksAmazingSubs_launch.log"
    log("Log file: " .. log_path)

    -- Get path to the Marks Amazing Subtitles app and modules.
    -- The installer writes the actual install path into install_path.txt inside this folder.
    local storage_path = os.getenv("APPDATA") ..
        "\\Blackmagic Design\\DaVinci Resolve\\Support\\Fusion\\Scripts\\Utility\\MarksAmazingSubs"
    log("Reading install_path.txt from: " .. storage_path)

    local ok, result = pcall(read_file, join_path(storage_path, "install_path.txt"))
    local install_path
    if ok then
        install_path = result:gsub("%s+$", "") -- trim trailing whitespace/newlines
        log("Install path from install_path.txt: " .. install_path)
    else
        -- install_path.txt not written (e.g. raw Tauri installer used instead of NSIS wrapper).
        -- Fall back to the default per-user Tauri install location.
        local fallback = os.getenv("LOCALAPPDATA") .. "\\Programs\\Marks Amazing Subtitles"
        log("install_path.txt not found, trying fallback: " .. fallback)
        local exe_check = io.open(fallback .. "\\autosubs.exe", "rb")
        if exe_check then
            exe_check:close()
            install_path = fallback
            log("Fallback install path found OK")
        else
            local err = "Could not find Marks Amazing Subtitles.\n" ..
                "Tried install_path.txt (" .. tostring(result) .. ")\n" ..
                "Tried fallback: " .. fallback .. "\\autosubs.exe\n" ..
                "Install the app and restart DaVinci Resolve."
            log("ERROR: " .. err)
            error(err)
        end
    end

    log("Install path: " .. install_path)

    app_executable = install_path .. "\\autosubs.exe"
    resources_folder = install_path .. "\\resources"

    log("App executable: " .. app_executable)
    log("Resources folder: " .. resources_folder)

    -- Verify the executable exists before trying to launch it
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

-- temporarily redefine path for dev_mode (replace with correct path to resources folder in repo)
if DEV_MODE then
    resources_folder = os.getenv("HOME") .. "/Documents/auto-subs/AutoSubs-App/src-tauri/resources"
end

log("Loading autosubs_core module from: " .. resources_folder)

-- Set package path for module loading
local modules_path = join_path(resources_folder, "modules")
package.path = package.path .. ";" .. join_path(modules_path, "?.lua")

-- Launch Marks Amazing Subtitles
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
