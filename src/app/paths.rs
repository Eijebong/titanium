/*
 * Copyright (c) 2016 Boucher, Antoni <bouanto@zoho.com>
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy of
 * this software and associated documentation files (the "Software"), to deal in
 * the Software without restriction, including without limitation the rights to
 * use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
 * the Software, and to permit persons to whom the Software is furnished to do so,
 * subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
 * FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
 * COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
 * IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
 * CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */

use std::path::PathBuf;

use app::App;
use config_dir::ConfigDir;

impl App {
    /// Get the config path of the bookmarks file.
    pub fn bookmark_path(config_dir: &ConfigDir) -> PathBuf {
        config_dir.config_file("bookmarks")
            .expect("cannot create configuration directory")
    }

    /// Get the config path of the password file.
    pub fn password_path(config_dir: &ConfigDir) -> PathBuf {
        config_dir.config_file("passwords")
            .expect("cannot create configuration directory")
    }

    /// Get the whitelist and blacklist path.
    pub fn popup_path(config_dir: &ConfigDir) -> (PathBuf, PathBuf) {
        ( config_dir.config_file("popups/whitelist")
            .expect("cannot create configuration directory")
        , config_dir.config_file("popups/blacklist")
            .expect("cannot create configuration directory")
        )
    }
}