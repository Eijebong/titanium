/*
 * Copyright (c) 2016-2018 Boucher, Antoni <bouanto@zoho.com>
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

//! Message server interface.

use std::collections::HashMap;
use std::marker;
use std::process;

use gio::{
    IOErrorEnum,
    IOStreamExt,
    Socket,
    SocketClient,
    SocketClientExt,
    SocketConnection,
    SocketExt,
    SocketFamily,
    SocketListener,
    SocketListenerExt,
    SocketProtocol,
    SocketType,
};
use glib;
use glib::Cast;
use gtk::{
    self,
    ButtonsType,
    DialogExt,
    DialogFlags,
    MessageDialog,
    MessageType,
    Window,
};
use relm::{Component, EventStream, Relm, Update, UpdateNew, execute, init};
use webkit2gtk::WebContext;

use titanium_common::{ExtensionId, InnerMessage, Message, PageId, SOCKET_NAME};
use titanium_common::gio_ext::{ListenerAsync, new_abstract_socket_address};
use titanium_common::InnerMessage::{Id, Open};
use titanium_common::protocol::{
    self,
    PluginProtocol,
    SendMode,
    send,
};
use titanium_common::protocol::Msg::{MsgRead, Write};

use app::{self, App};
use app::Msg::{
    MessageRecv,
    CreateWindow,
    Remove,
    ServerSend,
    SetPageId,
    ShowError,
};
use config_dir::ConfigDir;
use errors::{Error, Result};
use self::Msg::*;
use webview::WebView;

pub struct AppServer {
    stream: EventStream<app::Msg>,
    protocol: Option<EventStream<protocol::Msg>>,
}

impl AppServer {
    fn new(stream: EventStream<app::Msg>) -> Self {
        AppServer {
            stream,
            protocol: None,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Privacy {
    Normal,
    Private,
}

pub struct MessageServer {
    model: Model,
}

pub struct Model {
    app_count: usize,
    app_extensions: HashMap<PageId, (ExtensionId, usize)>,
    apps: HashMap<PageId, AppServer>,
    config_dir: ConfigDir,
    extension_page: HashMap<PageId, ExtensionId>,
    listener: SocketListener,
    private_web_context: WebContext,
    relm: Relm<MessageServer>,
    // TODO: save the widgets somewhere allowing to remove them when its window is closed.
    wins: Vec<Component<App>>,
    web_context: WebContext,
    protocol_counter: usize,
    protocols: HashMap<usize, EventStream<protocol::Msg>>,
}

#[derive(Msg)]
pub enum Msg {
    AppPageId(EventStream<app::Msg>, PageId),
    ClientConnect(SocketConnection),
    MsgRecv(usize, Message),
    MsgError(Error),
    NewApp(Option<String>, Privacy),
    RemoveApp(PageId),
    Send(PageId, InnerMessage),
}

// NOTE: safe because the main loop is ran on the main thread.
unsafe impl marker::Send for Msg {}

impl Update for MessageServer {
    type Model = Model;
    type ModelParam = (SocketListener, Vec<String>, Option<String>);
    type Msg = Msg;

    fn model(relm: &Relm<Self>, (listener, urls, config): (SocketListener, Vec<String>, Option<String>)) -> Model {
        let config_dir = ConfigDir::new(&config).unwrap(); // TODO: remove unwrap().
        let (web_context, private_web_context) = WebView::initialize_web_extension(&config_dir);
        if urls.is_empty() {
            relm.stream().emit(NewApp(None, Privacy::Normal));
        }
        else {
            for url in urls {
                relm.stream().emit(NewApp(Some(url), Privacy::Normal));
            }
        }
        Model {
            app_count: 0,
            app_extensions: HashMap::new(),
            apps: HashMap::new(),
            config_dir,
            extension_page: HashMap::new(),
            listener,
            private_web_context,
            relm: relm.clone(),
            wins: vec![],
            web_context,
            protocol_counter: 0,
            protocols: HashMap::new(),
        }
    }

    fn subscriptions(&mut self, _relm: &Relm<MessageServer>) {
        self.accept();
    }

    fn update(&mut self, event: Msg) {
        match event {
            AppPageId(stream, page_id) => {
                // FIXME: the writer is inserted too many times. It should be only once per web
                // extension, not once per page.
                self.model.apps.insert(page_id, AppServer::new(stream));
                if let Some((extension_id, protocol_counter)) = self.model.app_extensions.remove(&page_id) {
                    self.connect_app_and_extension(extension_id, page_id, protocol_counter);
                }
            },
            ClientConnect(stream) => {
                self.accept();
                let protocol = execute::<PluginProtocol>(stream.upcast());
                // TODO: check if it's possible to remove the protocols field.
                let counter = self.model.protocol_counter;
                self.model.protocol_counter += 1;
                connect_stream!(protocol@MsgRead(ref msg), self.model.relm.stream(), MsgRecv(counter, msg.clone()));
                // TODO: handle error.
                self.model.protocols.insert(counter, protocol);
            },
            // To be listened by the app.
            MsgError(_) => (),
            MsgRecv(protocol_counter, Message(page_id, message)) => self.msg_recv(protocol_counter, page_id, message),
            NewApp(url, privacy) => self.add_app(url, privacy),
            RemoveApp(page_id) => self.remove_app(page_id),
            Send(page_id, message) => self.send(page_id, message),
        }
    }
}

impl UpdateNew for MessageServer {
    fn new(_relm: &Relm<Self>, model: Self::Model) -> Self {
        MessageServer {
            model,
        }
    }
}

impl MessageServer {
    pub fn new(url: Vec<String>, config_dir: Option<String>) -> Result<EventStream<<Self as Update>::Msg>> {
        let listener = SocketListener::new();
        let address = new_abstract_socket_address(SOCKET_NAME);
        let socket = Socket::new(SocketFamily::Unix, SocketType::Stream, SocketProtocol::Default)?;
        if let Err(error) = socket.bind(&address, false) {
            if error.kind::<IOErrorEnum>() == Some(IOErrorEnum::AddressInUse) {
                info!("Address already in use for the abstract domain socket, sending message to existing process.");

                // A titanium process is already running, so we send the URL to this process so
                // that it can open a new window.
                // FIXME: this message is never sent (or received).
                if let Err(ref e) = send_url_to_existing_process(&url) {
                    println!("error: {}", e);

                    process::exit(1);
                }

                process::exit(0);
            }
            else {
                return Err(error.into());
            }
        }
        socket.listen()?;
        listener.add_socket(&socket, None::<&Socket>)?;
        Ok(execute::<MessageServer>((listener, url, config_dir)))
    }

    fn accept(&self) {
        connect_async_full!(ListenerAsync::new(&self.model.listener), accept_async, self.model.relm,
            |(connection, _)| ClientConnect(connection), |error: glib::Error| MsgError(error.into()));
    }

    fn add_app(&mut self, url: Option<String>, privacy: Privacy) {
        self.model.app_count += 1;
        let web_context =
            if privacy == Privacy::Private {
                self.model.private_web_context.clone()
            }
            else {
                self.model.web_context.clone()
            };
        let app = init::<App>((url, self.model.config_dir.clone(), web_context)).unwrap(); // TODO: remove unwrap().
        let app_stream = app.stream().clone();
        connect!(app@SetPageId(page_id), self.model.relm, AppPageId(app_stream.clone(), page_id));
        connect!(app@ServerSend(page_id, ref message), self.model.relm, Send(page_id, message.clone()));
        connect!(app@CreateWindow(ref url, ref privacy), self.model.relm, NewApp(Some(url.clone()), *privacy));
        connect!(app@Remove(page_id), self.model.relm, RemoveApp(page_id));
        self.model.wins.push(app);
    }

    fn connect_app_and_extension(&mut self, extension_id: ExtensionId, page_id: PageId, protocol_counter: usize) {
        if let Some(ref mut app) = self.model.apps.get_mut(&page_id) {
            trace!("Inserting page id {} in extension_page", page_id);
            self.model.extension_page.insert(page_id, extension_id);
            if let Some(protocol) = self.model.protocols.remove(&protocol_counter) {
                app.protocol = Some(protocol);
            }
        }
        else {
            error!("Cannot find app with page id {}", page_id);
        }
    }

    fn error(&self, page_id: PageId, error: Error) {
        if let Some(app) = self.model.apps.get(&page_id) {
            app.stream.emit(ShowError(error.to_string()));
        }
    }

    fn msg_recv(&mut self, protocol_counter: usize, page_id: PageId, msg: InnerMessage) {
        trace!("Receive message");
        if let Id(extension_id, page_id) = msg {
            trace!("Receive page id {}", page_id);
            if self.model.apps.contains_key(&page_id) {
                self.connect_app_and_extension(extension_id, page_id, protocol_counter);
            }
            else {
                self.model.app_extensions.insert(page_id, (extension_id, protocol_counter));
            }
        }
        else if let Open(urls) = msg {
            self.model.protocols.remove(&protocol_counter);
            if urls.is_empty() {
                self.add_app(None, Privacy::Normal);
            }
            else {
                for url in urls {
                    self.add_app(Some(url), Privacy::Normal);
                }
            }
        }
        else if let Some(ref app) = self.model.apps.get(&page_id) {
            app.stream.emit(MessageRecv(msg));
        }
        else {
            error!("Cannot find app with page id {}", page_id);
        }
    }

    fn remove_app(&mut self, page_id: PageId) {
        self.model.app_count -= 1;
        if let Some(extension_id) = self.model.extension_page.get(&page_id).cloned() {
            if page_id != extension_id {
                self.model.apps.remove(&page_id);
                trace!("Removing page id {} in extension_page", page_id);
                self.model.extension_page.remove(&page_id);
            }
            // TODO: remove the apps with extension ID? It seems web extensions are not recreated.
            // Is it because the webview is not destroyed?
        }
        // TODO: remove from self.model.wins.
        if self.model.app_count == 0 {
            gtk::main_quit();
        }
    }

    pub fn send(&mut self, page_id: PageId, message: InnerMessage) {
        let mut error = None;
        if let Some(extension_id) = self.model.extension_page.get(&page_id) {
            if let Some(app) = self.model.apps.get_mut(&extension_id) {
                if let Some(ref mut protocol) = app.protocol {
                    protocol.emit(Write(Message(page_id, message)));
                }
                else {
                    error = Some(Error::new("message protocol does not exist"));
                }
            }
            else {
                error = Some(Error::new("app does not exist"));
            }
        }
        else {
            error = Some(Error::from_string(format!("extension id for page {} does not exist", page_id)));
        }
        if let Some(error) = error {
            self.error(page_id, error);
        }
    }
}

/// Create a new message server.
/// If it is not possible to create one, show the error and exit.
pub fn create_message_server(url: Vec<String>, config_dir: Option<String>) -> EventStream<<MessageServer as Update>::Msg> {
    match MessageServer::new(url, config_dir) {
        Ok(message_server) => message_server,
        Err(error) => {
            let message = format!("cannot create the message server used to communicate with the web processes: {}",
                error);
            dialog_and_exit(&message);
        },
    }
}

fn dialog_and_exit(message: &str) -> ! {
    let window: Option<&Window> = None;
    let message = format!("Fatal error: {}", message);
    let dialog = MessageDialog::new(window, DialogFlags::empty(), MessageType::Error, ButtonsType::Close, &message);
    dialog.run();
    process::exit(1);
}

fn send_url_to_existing_process(url: &[String]) -> Result<()> {
    let client = SocketClient::new();
    let address = new_abstract_socket_address(SOCKET_NAME);
    let connection = client.connect(&address, None)?;
    let writer = connection.get_output_stream().ok_or_else(|| "cannot get output stream")?;
    send(&writer, Message(0, Open(url.to_vec())), SendMode::Sync);
    Ok(())
}
