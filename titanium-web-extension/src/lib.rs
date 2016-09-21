
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

/*
 * TODO: switch from AtomicIsize to AtomicU64.
 */

extern crate dbus;
#[macro_use]
extern crate dbus_macros;
#[macro_use]
extern crate webkit2gtk_webextension;

use std::sync::Arc;
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::Ordering::Relaxed;
use std::thread;

use glib::variant::Variant;
use webkit2gtk_webextension::{DOMDocumentExt, DOMElement, DOMElementExt, DOMHTMLElement, WebExtension, WebPage};

web_extension_init!();

dbus_class!("com.titanium.client", class MessageServer (page_id: Arc<AtomicIsize>, extension: WebExtension) {
    fn get_scroll_percentage(&this) -> i64 {
        if let Some(page) = this.extension.get_page(this.page_id.load(Relaxed) as u64) {
            scroll_percentage(&page)
        }
        else {
            0
        }
    }

    fn scroll_bottom(&this) {
        if let Some(page) = this.extension.get_page(this.page_id.load(Relaxed) as u64) {
            scroll_bottom(&page);
        }
    }

    fn scroll_by(&this, pixels: i64) {
        if let Some(page) = this.extension.get_page(this.page_id.load(Relaxed) as u64) {
            scroll_by(&page, pixels);
        }
    }

    fn scroll_top(&this) {
        if let Some(page) = this.extension.get_page(this.page_id.load(Relaxed) as u64) {
            scroll_top(&page);
        }
    }
});

#[no_mangle]
pub fn web_extension_initialize(extension: WebExtension, user_data: Variant) {
    let current_page_id = Arc::new(AtomicIsize::new(-1));

    {
        let current_page_id = current_page_id.clone();
        extension.connect_page_created(move |_, page| {
            current_page_id.store(page.get_id() as isize, Relaxed);
        });
    }

    let bus_name = user_data.get_str();
    if let Some(bus_name) = bus_name {
        let bus_name = bus_name.to_string();
        let message_server = MessageServer::new(current_page_id, extension);
        thread::spawn(move || message_server.run(&bus_name));
    }
}

/// Get the body element of the web page.
fn get_body(page: &WebPage) -> Option<DOMHTMLElement> {
    page.get_dom_document().and_then(|document|
        document.get_body()
    )
}

/// Get the document element of the web page.
fn get_document(page: &WebPage) -> Option<DOMElement> {
    page.get_dom_document().and_then(|document|
        document.get_document_element()
    )
}

/// Scroll the web page vertically by the specified amount of pixels.
/// A negative value scroll towards to top.
fn scroll_by(page: &WebPage, pixels: i64) {
    if let Some(body) = get_body(page) {
        body.set_scroll_top(body.get_scroll_top() + pixels);
    }
}

/// Scroll to the bottom of the web page.
fn scroll_bottom(page: &WebPage) {
    if let Some(body) = get_body(page) {
        body.set_scroll_top(body.get_scroll_height());
    }
}

/// Get the current vertical scroll position of the web page as a percentage.
fn scroll_percentage(page: &WebPage) -> i64 {
    let default = -1;
    if let (Some(body), Some(document)) = (get_body(page), get_document(page)) {
        let height = document.get_client_height();
        let scroll_height = body.get_scroll_height();
        if scroll_height <= height as i64 {
            default
        }
        else {
            (body.get_scroll_top() as f64 / (scroll_height as f64 - height) * 100.0) as i64
        }
    }
    else {
        default
    }
}

/// Scroll to the top of the web page.
fn scroll_top(page: &WebPage) {
    if let Some(body) = get_body(page) {
        body.set_scroll_top(0);
    }
}
