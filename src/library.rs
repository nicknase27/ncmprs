use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use audiotags::Tag;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use config::{Config, File};

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub library_path: PathBuf,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Song {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Option<f64>,
    pub path: PathBuf,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Library {
    pub songs: Vec<Song>,
    #[serde(skip)]
    pub artists: Vec<String>,
    #[serde(skip)]
    pub artist_albums: HashMap<String, BTreeSet<String>>,
    #[serde(skip)]
    pub album_songs: HashMap<(String, String), Vec<usize>>,
}

impl Library {
    pub fn new() -> Self {
    let lib_path = Self::library_path();

    if lib_path.exists() {
        Self::load()
    } else {
        let library = Self::scan();
        library.save();
        library
    }
}

    pub fn scan() -> Self {
        let settings = Self::config();
        let temp_songs = get_songs(&settings.library_path);
        let mut songs: Vec<Song> = vec![];

        for (index, song) in temp_songs.into_iter().enumerate() {
            let tag = Tag::default().read_from_path(&song).expect("Error");
            let song = Song {
                title: tag.title().unwrap_or("Could not read title").to_string(),
                artist: tag.artist().unwrap_or("Could not read artist").to_string(),
                album: tag.album().unwrap().title.to_string(),
                duration: if tag.duration().is_some() {
                    tag.duration()
                } else {
                    Some(0.1)
                },
                path: song,
            };

            songs.push(song);
        }

        let mut library = Self {
            songs,
            artists: Vec::new(),
            artist_albums: HashMap::new(),
            album_songs: HashMap::new(),
        };

        library.rebuild_indexes();

        library
    }

    pub fn load() -> Self {
        let lib_path = Self::library_path();
        let json = std::fs::read_to_string(lib_path).unwrap();
        let mut library: Library = serde_json::from_str(&json).unwrap();

        library.rebuild_indexes();

        library
    }

    pub fn albums_for_artist(&self, artist: &String) -> Vec<String> {
        self.artist_albums
            .get(artist)
            .unwrap()
            .iter()
            .cloned()
            .collect()
    }

    pub fn songs_for_album(&self, artist: &String, album: &String) -> &Vec<usize> {
        self.album_songs
            .get(&(artist.clone(), album.clone()))
            .unwrap()
    }

    pub fn save(&self) {
        let lib_path = Self::library_path();
        std::fs::create_dir_all(lib_path.parent().unwrap()).unwrap();

        let json = serde_json::to_string(self).unwrap();
        std::fs::write(lib_path, json).unwrap();
    }

    fn rebuild_indexes(&mut self) {
        self.artists.clear();
        self.artist_albums.clear();
        self.album_songs.clear();

        let mut artists = BTreeSet::new();

        for (index, song) in self.songs.iter().enumerate() {
            artists.insert(song.artist.clone());

            self.artist_albums
                .entry(song.artist.clone())
                .or_default()
                .insert(song.album.clone());

            self.album_songs
                .entry((song.artist.clone(), song.album.clone()))
                .or_default()
                .push(index);
        }

        self.artists = artists.into_iter().collect();
    }

    fn library_path() -> PathBuf {
        dirs::cache_dir()
            .expect("Cache directory does not exist")
            .join("ncmprs")
            .join("library.json")
    }

    fn config() -> Settings {
        load_config().expect("Failed to load config.toml")
    }
}

/* 
fn get_songs() -> Vec<PathBuf> {
    let mut songs: Vec<PathBuf> = vec![];

    
    let flacs = WalkDir::new("/mnt/media/music")
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_type().is_file() && e.path().extension().is_some_and(|ext| ext == "flac")
        })
        .map(|e| e.into_path());

    for path in flacs {
        // `path` is a PathBuf
        //println!("{}", path.display());
        songs.push(path);
    }
    songs.sort();
    songs
}
*/

fn get_songs(library: &Path) -> Vec<PathBuf> {
    let mut songs: Vec<PathBuf> = WalkDir::new(library)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_type().is_file()
                && e.path()
                    .extension()
                    .is_some_and(|ext| ext == "flac")
        })
        .map(|e| e.into_path())
        .collect();

    songs.sort();
    songs
}

pub fn load_config() -> Result<Settings, config::ConfigError> {
    let config_path = dirs::config_dir()
        .expect("Couldn't determine config directory")
        .join("ncmprs")
        .join("config.toml");

    Config::builder()
        .add_source(File::from(config_path))
        .build()?
        .try_deserialize()
}

