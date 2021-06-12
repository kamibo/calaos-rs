use std::convert::From;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Event {
    Unknown,
    IOAdded,
    IODeleted,
    IOChanged,
    IOPropertyDelete,
    RoomAdded,
    RoomDeleted,
    RoomChanged,
    RoomPropertyDelete,
    TimeRangeChanged,
    ScenarioAdded,
    ScenarioDeleted,
    ScenarioChanged,
    AudioSongChanged,
    AudioPlaylistAdd,
    AudioPlaylistDelete,
    AudioPlaylistMove,
    AudioPlaylistReload,
    AudioPlaylistCleared,
    AudioStatusChanged,
    AudioVolumeChanged,
    TouchScreenCamera,
    PushNotification,
}

impl From<Event> for &'static str {
    fn from(value: Event) -> Self {
        match value {
            Event::Unknown => "unknown",
            Event::IOAdded => "io_added",
            Event::IODeleted => "io_deleted",
            Event::IOChanged => "io_changed",
            Event::IOPropertyDelete => "io_prop_deleted",

            Event::RoomAdded => "room_added",
            Event::RoomDeleted => "room_deleted",
            Event::RoomChanged => "room_changed",
            Event::RoomPropertyDelete => "room_prop_deleted",

            Event::TimeRangeChanged => "timerange_changed",
            Event::ScenarioAdded => "scenario_added",
            Event::ScenarioDeleted => "scenario_deleted",
            Event::ScenarioChanged => "scenario_changed",

            Event::AudioSongChanged => "audio_song_changed",
            Event::AudioPlaylistAdd => "playlist_tracks_added",
            Event::AudioPlaylistDelete => "playlist_tracks_deleted",
            Event::AudioPlaylistMove => "playlist_tracks_moved",
            Event::AudioPlaylistReload => "playlist_reload",
            Event::AudioPlaylistCleared => "playlist_cleared",

            Event::AudioStatusChanged => "audio_status_changed",
            Event::AudioVolumeChanged => "audio_volume_changed",

            Event::TouchScreenCamera => "touchscreen_camera_request",

            Event::PushNotification => "push_notif",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_event_value() {
        assert_eq!(Event::IOChanged as i32, 3);
        let s: &'static str = From::from(Event::IOChanged);
        assert_eq!(s, "io_changed");
    }
}
