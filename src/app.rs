use std::collections::VecDeque;
use std::fs::File;
use std::io::BufReader;

use crate::event::{AppEvent, Event, EventHandler};
use crate::library::{Library, Song};

use audiotags::{MimeType, Tag};
use crossterm::event::MediaKeyCode::{self, PlayPause};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use discord_rich_presence::activity::{Assets, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient, activity};
use ratatui::DefaultTerminal;
use ratatui::widgets::{List, ListItem, ListState};
use rodio::Decoder;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Default, PartialEq, Eq)]
pub enum Focus {
    #[default]
    Artists,
    Albums,
    Songs,
    Queue,
}

/// Application.
//#[derive(Debug)]
pub struct App {
    /// Is the application running?
    pub running: bool,

    pub library: Library,

    pub queue: VecDeque<usize>,

    pub sink_handle: rodio::MixerDeviceSink,
    pub player: rodio::Player,

    pub artist_state: ListState,
    pub album_state: ListState,
    pub song_state: ListState,
    pub queue_state: ListState,
    pub focus: Focus,

    pub current_song: Option<usize>,
    pub paused_at: i64,

    pub discord_client: Option<DiscordIpcClient>,

    /// Event handler.
    pub events: EventHandler,
}

impl App {
    /// Constructs a new instance of [`App`].
    pub fn new() -> Self {
        let sink_handle =
            rodio::DeviceSinkBuilder::open_default_sink().expect("open default audio stream");
        let player = rodio::Player::connect_new(sink_handle.mixer());
        player.set_volume(1.0);

        let discord_client = {
            let mut client = DiscordIpcClient::new("1525954755292299426");

            match client.connect() {
                Ok(_) => Some(client),
                Err(e) => {
                    eprintln!("Discord RPC unavailable: {e}");
                    None
                }
            }
        };

        Self {
            running: true,
            library: Library::new(),
            //songs: get_songs(),
            queue: vec![].into(),
            artist_state: ListState::default().with_selected(Some(0)),
            album_state: ListState::default().with_selected(Some(0)),
            song_state: ListState::default().with_selected(Some(0)),
            queue_state: ListState::default().with_selected(Some(0)),
            focus: Focus::default(),
            events: EventHandler::new(),
            current_song: None,
            paused_at: 1,
            sink_handle,
            player,
            discord_client,
        }
    }

    /// Run the application's main loop.
    pub async fn run(mut self, mut terminal: DefaultTerminal) -> color_eyre::Result<()> {
        while self.running {
            terminal.draw(|frame| frame.render_widget(&mut self, frame.area()))?;
            match self.events.next().await? {
                Event::Tick => self.tick(),
                Event::Crossterm(event) => match event {
                    crossterm::event::Event::Key(key_event)
                        if key_event.kind == crossterm::event::KeyEventKind::Press =>
                    {
                        self.handle_key_events(key_event)?
                    }
                    _ => {}
                },
                Event::App(app_event) => match app_event {
                    AppEvent::Play => match self.focus {
                        Focus::Artists => (),
                        Focus::Albums => self.enqueue_album(),
                        Focus::Songs => self.play_song(self.song_state.selected().expect("None")),
                        _ => (),
                    },
                    AppEvent::Skip => self.skip(),
                    AppEvent::PlayPause => self.play_pause(),
                    AppEvent::IncVolume => self.inc_volume(),
                    AppEvent::DecVolume => self.dec_volume(),
                    AppEvent::MoveRight => self.move_right(),
                    AppEvent::MoveLeft => self.move_left(),
                    AppEvent::RemoveFromQueue => self.remove_from_queue(),

                    AppEvent::Quit => self.quit(),
                },
            }
        }
        drop(self.discord_client);
        Ok(())
    }

    /// Handles the key events and updates the state of [`App`].
    pub fn handle_key_events(&mut self, key_event: KeyEvent) -> color_eyre::Result<()> {
        match key_event.code {
            KeyCode::Esc | KeyCode::Char('q') => self.events.send(AppEvent::Quit),
            KeyCode::Char('c' | 'C') if key_event.modifiers == KeyModifiers::CONTROL => {
                self.events.send(AppEvent::Quit)
            }

            KeyCode::Char('R') => {
                self.library = Library::scan();
                self.library.save();
            }

            KeyCode::Char('l') | KeyCode::Right | KeyCode::Tab => {
                self.events.send(AppEvent::MoveRight)
            }
            KeyCode::Char('h') | KeyCode::Left => self.events.send(AppEvent::MoveLeft),

            KeyCode::Char('j') | KeyCode::Down => match self.focus {
                Focus::Artists => self.artist_state.select_next(),
                Focus::Albums => self.album_state.select_next(),
                Focus::Songs => self.song_state.select_next(),
                Focus::Queue => self.queue_state.select_next(),
            },
            KeyCode::Char('k') | KeyCode::Up => match self.focus {
                Focus::Artists => self.artist_state.select_previous(),
                Focus::Albums => self.album_state.select_previous(),
                Focus::Songs => self.song_state.select_previous(),
                Focus::Queue => self.queue_state.select_previous(),
            },

            KeyCode::Char('g') => match self.focus {
                Focus::Artists => self.artist_state.select_first(),
                Focus::Albums => self.album_state.select_first(),
                Focus::Songs => self.song_state.select_first(),
                Focus::Queue => self.queue_state.select_first(),
            },
            KeyCode::Char('G') => match self.focus {
                Focus::Artists => self.artist_state.select_last(),
                Focus::Albums => self.album_state.select_last(),
                Focus::Songs => self.song_state.select_last(),
                Focus::Queue => self.queue_state.select_last(),
            },

            KeyCode::Enter => self.events.send(AppEvent::Play),
            KeyCode::Char('x') => self.stop_playback(),
            KeyCode::Char(' ') | KeyCode::Media(MediaKeyCode::PlayPause) => {
                self.events.send(AppEvent::PlayPause)
            }
            KeyCode::Char('+') | KeyCode::Char('=') => self.events.send(AppEvent::IncVolume),
            KeyCode::Char('-') | KeyCode::Char('_') => self.events.send(AppEvent::DecVolume),
            KeyCode::Delete => self.events.send(AppEvent::RemoveFromQueue),
            KeyCode::Char('>') => self.events.send(AppEvent::Skip),
            // Other handlers you could add here.
            _ => {}
        }
        Ok(())
    }

    /*
    pub fn play(&mut self) {
            let file = File::open(self.songs[self.list_state.selected().expect("None")].clone()).unwrap();
            let source = Decoder::new(BufReader::new(file)).unwrap();

            if self.player.empty() {
                self.player.append(source);
            } else {
                self.player.stop();
                self.player.append(source);
            }

    }
    */
    pub fn play(&mut self) {
        if !self.queue.is_empty() {
            for _i in 0..self.queue.len() {
                let file = File::open(
                    self.library.songs[*self.queue.front().expect("None")]
                        .path
                        .clone(),
                )
                .unwrap();
                let source = Decoder::new(BufReader::new(file)).unwrap();
                self.queue.pop_front();
                self.player.append(source);
            }
        } else {
            let file = File::open(
                self.library.songs[self.artist_state.selected().expect("None")]
                    .path
                    .clone(),
            )
            .unwrap();
            let source = Decoder::new(BufReader::new(file)).unwrap();
            if self.player.empty() {
                self.player.append(source);
            } else {
                self.player.stop();
                self.player.append(source);
            }
        }
    }

    pub fn play_song(&mut self, index: usize) {
        let song_indexes = self.current_selected_song();
        let song_index = song_indexes[index];

        self.queue.push_back(song_index);
    }

    pub fn enqueue_album(&mut self) {
        let song_indexes = self.current_selected_song();

        for &song in &song_indexes {
            self.queue.push_back(song);
        }
    }

    pub fn current_selected_song(&mut self) -> Vec<usize> {
        let selected_artist = &self.library.artists[self.artist_state.selected().unwrap()];
        let albums = self.library.albums_for_artist(selected_artist);
        let selected_album = &albums[self.album_state.selected().unwrap()];
        let song_indexes = self
            .library
            .songs_for_album(selected_artist, selected_album);
        song_indexes.to_owned()
    }

    pub fn play_pause(&mut self) {
        if self.current_song().is_some() {
            let song = &self.library.songs[self.current_song.unwrap()];
            let duration = song.duration.unwrap_or(1.0) as i64;
            let artist = &song.artist;
            if let Some(client) = self.discord_client.as_mut() {
                if !self.player.is_paused() {
                    self.player.pause();
                    self.paused_at = self.player.get_pos().as_secs() as i64;

                    let asset = Assets::new();
                    let activity = activity::Activity::new()
                    .name("NCMPRS")
                    .details(song.title.clone())
                    .state("Paused")
                    .activity_type(activity::ActivityType::Listening)
                    .assets(asset.large_image("https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/png/navidrome.png"));
                    let _ = client.set_activity(activity);
                } else if self.player.is_paused() {
                    self.player.play();
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64;
                    let timestamps = Timestamps::new()
                        .start(now - self.paused_at)
                        .end((now - self.paused_at) + duration);
                    let asset = Assets::new();

                    let payload = activity::Activity::new()
                    .name("NCMPRS")
                    .details(song.title.clone())
                    .state(artist)
                    .activity_type(activity::ActivityType::Listening)
                    .timestamps(timestamps)
                    .assets(asset.large_image("https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/png/navidrome.png"));

                    let _ = client.set_activity(payload);
                }
            }
        }

        /*
        if !self.player.is_paused() {
            self.player.pause();

            self.paused_at = self.player.get_pos().as_secs() as i64;
            if let Some(client) = self.discord_client.as_mut() {
                let song = &self.library.songs[self.current_song.unwrap()];
                //let duration = song.duration.unwrap_or(1.0) as i64;
                //let artist = &song.artist;
                let asset = Assets::new();
                let activity = activity::Activity::new()
                    .name("NCMPRS")
                    .details(song.title.clone())
                    .state("Paused")
                    .activity_type(activity::ActivityType::Listening)
                    .assets(asset.large_image("https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/png/navidrome.png"));
                let _ = client.set_activity(activity);
            }
        } else if self.player.is_paused() {
            self.player.play();
            let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
            if let Some(client) = self.discord_client.as_mut() {
                let song = &self.library.songs[self.current_song.unwrap()];
                let duration = song.duration.unwrap_or(1.0) as i64;
                let artist = &song.artist;
                let timestamps = Timestamps::new()
            .start(now - self.paused_at)
            .end((now - self.paused_at) + duration);
        let asset = Assets::new();


        let payload = activity::Activity::new()
            .name("NCMPRS")
            .details(song.title.clone())
            .state(artist)
            .activity_type(activity::ActivityType::Listening)
            .timestamps(timestamps)
            .assets(asset.large_image("https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/png/navidrome.png"));

        let _ = client.set_activity(payload);
            }
        }*/
    }

    pub fn inc_volume(&mut self) {
        let vol = self.player.volume();
        //self.player.set_volume(self.player.volume() + 0.1);
        if vol >= 1.0 {
            self.player.set_volume(1.0);
        } else {
            self.player.set_volume(vol + 0.05);
        }
    }

    pub fn dec_volume(&mut self) {
        let vol = self.player.volume();
        if vol <= 0.01 {
            self.player.set_volume(0.0);
        } else {
            self.player.set_volume(vol - 0.05);
        }
    }

    pub fn skip(&mut self) {
        self.player.skip_one();
        if self.queue.is_empty() {
            self.current_song = None;

            if let Some(client) = self.discord_client.as_mut() {
                let _ = client.clear_activity();
            }
        }
    }

    pub fn remove_from_queue(&mut self) {
        if self.queue_state.selected().is_some() {
            self.queue.remove(self.queue_state.selected().unwrap());
        }
    }

    pub fn stop_playback(&mut self) {
        self.queue.clear();
        self.player.stop();
        if let Some(client) = self.discord_client.as_mut() {
            let _ = client.clear_activity();
        }
        self.current_song = None;
    }

    pub fn move_right(&mut self) {
        match self.focus {
            Focus::Artists => self.focus = Focus::Albums,
            Focus::Albums => self.focus = Focus::Songs,
            Focus::Songs => self.focus = Focus::Queue,
            Focus::Queue => self.focus = Focus::Artists,
        }
    }

    pub fn move_left(&mut self) {
        match self.focus {
            Focus::Artists => self.focus = Focus::Queue,
            Focus::Albums => self.focus = Focus::Artists,
            Focus::Songs => self.focus = Focus::Albums,
            Focus::Queue => self.focus = Focus::Songs,
        }
    }

    pub fn current_song(&self) -> Option<&Song> {
        self.current_song.map(|i| &self.library.songs[i])
    }

    pub fn current_song_title(&self) -> &str {
        self.current_song().map(|s| s.title.as_str()).unwrap_or("")
    }

    pub fn update_rpc(&mut self, index: usize) {
        if let Some(client) = self.discord_client.as_mut() {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;

            let song = &self.library.songs[index];
            let duration = song.duration.unwrap_or(1.0) as i64;
            let artist = &song.artist;

            let timestamps = Timestamps::new().start(now).end(now + duration);
            let asset = Assets::new();

            let payload = activity::Activity::new()
                .name("NCMPRS")
                .details(song.title.clone())
                .state(artist)
                .activity_type(activity::ActivityType::Listening)
                .timestamps(timestamps)
                .assets(asset.large_image(
                    "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/png/navidrome.png",
                ));

            let _ = client.set_activity(payload);
        }
    }

    /// Handles the tick event of the terminal.
    ///
    /// The tick event is where you can update the state of your application with any logic that
    /// needs to be updated at a fixed frame rate. E.g. polling a server, updating an animation.
    pub fn tick(&mut self) {
        if self.player.empty() && !self.queue.is_empty() {
            let file =
                File::open(&self.library.songs[*self.queue.front().expect("Queue is empty")].path)
                    .unwrap();
            let source = Decoder::new(BufReader::new(file)).unwrap();
            self.player.append(source);
            self.current_song = Some(self.queue.front().unwrap().to_owned());
            self.update_rpc(self.queue.front().unwrap().to_owned());
            self.queue.pop_front();
        }
    }

    /// Set running to false to quit the application.
    pub fn quit(&mut self) {
        self.running = false;
    }
}
