# Archiplayer user manual

Archiplayer is a desktop player for the WFMU archives. It keeps your favourites, history and settings on your computer. There is no login and no cloud.

## Contents

1. [Start](#1-start)
2. [Browse and search](#2-browse-and-search)
3. [Play](#3-play)
4. [Favourites and saved items](#4-favourites-and-saved-items)
5. [Song start and end](#5-song-start-and-end)
6. [Downloads](#6-downloads)
7. [History and statistics](#7-history-and-statistics)
8. [Export your data](#8-export-your-data)
9. [Keyboard shortcuts](#9-keyboard-shortcuts)
10. [Colours and window size](#10-colours-and-window-size)
11. [Problems](#11-problems)

## 1. Start

### Install

1. Download the installer (`.exe`) from the [latest release](https://github.com/peqod/Archiplayer/releases/latest).
2. Open the installer. The build is not signed, so Windows SmartScreen shows a warning.
3. Select **More info**, make sure that the file came from this repository, then select **Run anyway**.

To make sure that the file is correct, compare its SHA-256 hash with `SHA256SUMS.txt` on the release page.

### Where your data is

All your data is in one SQLite file:

```
%APPDATA%\org.archiplayer.app\library.db
```

This file contains your favourites, your listening history, your song marks and the cached show data. Nothing goes to a server.

## 2. Browse and search

### Shows

The home page shows all WFMU shows, current and old, with an A-Z index. Select a show to open its page. The show page shows the episodes, newest first. Select an episode to open its playlist.

### Search

Type in the search field on the home page.

- **Shows and DJs**: results show immediately.
- **Songs**: the search looks in all the playlists that the app has already loaded. A playlist loads when you open its episode. Thus, the more episodes you open, the more songs you can find.

### Random show

Select **I'm feeling lucky today** in the empty player (or press `R`). The app plays a random episode.

## 3. Play

### What you can play

- **An episode**: select the play button of the episode.
- **One song**: select the play button of a song in a playlist. Playback starts at the song's time in the playlist. This is possible only when the playlist has times.
- **A list of favourite songs**: see [Favourites](#4-favourites-and-saved-items).
- **Live**: select a live stream to hear WFMU now.

### The player bar

The player bar stays at the top of the window. It shows:

- the show name and the date of the episode, with a **save** button for the episode;
- the song that plays now, with a **star** button for the song;
- the seek bar, the time and the volume.

When a favourite song plays, two more buttons show next to the song name: the **flag** (mark the end) and the **range** (edit the start and end). See [Song start and end](#5-song-start-and-end).

### Seek bar

- Click the bar to go to that position.
- Press and drag to move with high precision. A magnifier shows the area below the pointer.
- Small marks on the bar show where the songs of the playlist start.

### Resume

The app remembers the position in each episode. When you play the episode again, it continues from that position.

### Queue

When you play a list (episodes or favourite songs), the app plays the items one after the other.

- **Previous / next song**: `J` / `K`.
- **Previous / next episode**: `,` / `.`.
- **Shuffle** and **repeat**: use the buttons above the lists in the Profile page.

## 4. Favourites and saved items

You can keep three types of items:

| Item | How to keep it | Where it shows |
|---|---|---|
| Show | Select the save button on the show page or in the search results | Profile > **Shows** |
| Episode | Select the **save** button next to the episode (or in the player bar, or press `S`) | Profile > **Episodes** |
| Song | Select the **star** next to the song in a playlist (or in the player bar, or press `F`) | Profile > **Songs** |

To remove an item, select the same button again, or select the filled star in the Profile list.

### Songs starred from a live stream

A live stream has no recording yet, so a song that you star there cannot play at first. The Songs list shows "from a live stream, waiting for the archive".

Each time the app starts, and each time you open Profile, it looks for these songs in the WFMU archive. While it looks, the Songs list shows "checking the archive...":

1. It checks if the archive has the recording of the live broadcast.
2. It finds the archived episode of the same show and day.
3. It finds the same song (artist and title) in the playlist of that episode.

When it finds the song, it moves your favourite to that playlist row and shows a message. The Songs list updates by itself. The song then plays from the archive. WFMU usually adds a broadcast to the archive within a few days. The app looks for each song again at most every 6 hours.

### Play your favourite songs

1. Open **Profile > Songs**.
2. Select the play button of a song.

The app plays that song and then all the songs below it in the list. Each song stops at its end (see the next section). Then the next song starts.

## 5. Song start and end

A favourite song plays from its **start** to its **end**. The app finds these points automatically. You can also set them yourself. Your own values always win.

### Automatic start

| Playlist | Start |
|---|---|
| The playlist has times | The time of the song in the playlist. |
| The playlist has no times, and you starred the song while its episode played | The position of the player at the moment when you selected the star. |
| The playlist has no times, and the episode did not play when you starred the song | The start of the episode. The Songs list shows "start not set". |

**Tip for playlists without times:** play the episode. When you hear the song, select its star in the playlist on the show page. The app saves that moment as the start of the song.

### Automatic end

The song stops at the first of these points:

1. the time of the next song in the playlist;
2. the length of the recording (from MusicBrainz) plus 5 seconds.

If the app does not know either point, the song plays to the end of the episode. When the start is "not set", only your own end mark applies.

### Set the start and end in the profile

1. Open **Profile > Songs**.
2. Select the **range** button (two brackets) of the song.
3. Type the start in **Starts** and the end in **ends**. Use `m:ss` or `h:mm:ss`, for example `4:05` or `1:12:30`. The times are from the start of the show, the same times that the playlist shows.
4. Select **Save** (or press Enter). Select **Cancel** (or press Esc) to stop without a change.

An empty field means "automatic". To remove your own start or end, delete the text in the field and save.

The end must be at least 10 seconds after the start.

### Set the start and end in the player

1. Play the song from **Profile > Songs**.
2. Select the **range** button next to the song name (or press `B`).
3. The seek bar changes to a range: a coloured part with a handle at each side. A thin line shows the player position.
4. Drag the left handle to the start, and the right handle to the end. The times show above the bar.
5. Select **Save**. Select **Cancel** (or press Esc) to stop without a change.

A handle that you do not move keeps its old value. The new end applies immediately. The new start applies the next time the song starts.

### Mark the end while you listen

When the song ends, select the **flag** button next to the song name (or press `E`). The app saves the position of the player as the end of the song.

## 6. Downloads

You can download an episode to listen to it without an internet connection.

1. Select the **download** button of the episode on the show page.
2. Open **Profile > Downloads** to see the progress and the files.

In **Profile > Downloads** you can:

- play the local copy;
- show the file in File Explorer;
- delete the file;
- change the download folder.

When a downloaded episode plays, the app uses the local file.

## 7. History and statistics

The app records how long you listen to each episode. The top of the **Profile** page shows:

- your total listening time;
- your listening activity by day;
- the shows that you listen to most;
- the archive years that you listen to.

## 8. Export your data

At the top of the **Profile** page, **Export CSV** has three buttons. Each one asks where to save a `.csv` file.

| Button | File content |
|---|---|
| **Favourites** | All saved shows, episodes and songs: `kind`, `name`, `detail`, `show`, `date_added`. |
| **History** | Each listening session: `session`, `show`, `episode_date`, `episode_title`, `started_at`, `seconds`, `completed`. |
| **Ranking** | Your shows by listening time: `rank`, `show`, `seconds_listened`, `hours`, `plays`. |

You can open these files in a spreadsheet program.

### Full backup

The CSV files do not contain everything (for example, song marks). For a full backup:

1. Close Archiplayer.
2. Copy `%APPDATA%\org.archiplayer.app\library.db` to a safe location.

To restore, close the app and put the copy back in the same folder.

## 9. Keyboard shortcuts

There are two sets of shortcuts:

- **In the app**: they work when the Archiplayer window has focus.
- **System-wide**: they work also when a different program has focus.

| Action | In the app | System-wide |
|---|---|---|
| Play / pause | `Space` | (media keys) |
| Previous / next song | `J` / `K` | (media keys) |
| Previous / next episode | `,` / `.` | `Ctrl+Shift+F6` / `Ctrl+Shift+F7` |
| Mute | `M` | `Ctrl+Shift+F8` |
| Star this song | `F` | `Ctrl+Shift+F9` |
| Mark song end | `E` | none |
| Edit song range | `B` | none |
| Save this episode | `S` | `Ctrl+Shift+F10` |
| Random audition | `R` | `Ctrl+Shift+F11` |

To change a shortcut, open **Profile > Keyboard shortcuts**, select the shortcut, then press the new keys. One switch there turns all system-wide shortcuts off.

When you type in a text field, the keys go to the field and not to the shortcuts.

## 10. Colours and window size

- **Colours**: open **Profile > Customize**. Select a colour scheme, or change each colour. The app keeps your choice.
- **Compact player**: you can collapse the window to a small player that shows only the player bar.

## 11. Problems

| Message | Meaning and what to do |
|---|---|
| "No archived audio" | WFMU has no recording of this episode. You cannot play it. |
| "from a live stream, waiting for the archive" | You starred the song from a live stream. The app moves it to the archive when WFMU archives the broadcast. See [Songs starred from a live stream](#songs-starred-from-a-live-stream). |
| "start not set" | The playlist has no times. The song plays from the start of the episode. Set the start: see [Song start and end](#5-song-start-and-end). |
| "No connection." | The internet connection stopped. The app tries again when the connection is back. Downloaded episodes still play. |
| Search does not find a song | The playlist of that episode is not loaded yet. Open the episode once. |

To report a problem, open an issue on [GitHub](https://github.com/peqod/Archiplayer/issues/new/choose).
