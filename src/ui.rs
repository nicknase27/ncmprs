use ratatui::{
    buffer::Buffer, layout::{
        Alignment, Constraint,
        Direction::{self},
        Layout, Rect,
    }, style::{Color, Modifier, Style}, symbols, widgets::{
        Block, BorderType::{self, Rounded}, LineGauge, List, ListDirection, ListItem, Paragraph, StatefulWidget, Widget,
    },
};

use crate::app::App;
use crate::app::Focus;

impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let position = self.player.get_pos().as_secs_f64();

        let outer_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(50),
                Constraint::Percentage(25),
            ])
            .split(area);

        let left_area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(100)])
            .split(outer_layout[0]);

        let right_area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(outer_layout[2]);

        let center_area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(8), // Album list
                Constraint::Length(6),
            ])
            .split(outer_layout[1]);

        let now_playing_block = Block::bordered().title(format!("Now Playing | Vol: {:.0}", self.player.volume() * 100.0)).border_type(Rounded);
        let inner = now_playing_block.inner(center_area[1]);
        Widget::render(now_playing_block, center_area[1], buf);

        let player_area = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(4),
            Constraint::Fill(1),
        ])
        .split(inner);

        let now_playing = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(player_area[1]);

        let progress = Layout::horizontal([
            Constraint::Length(3),  // ▶
            Constraint::Length(2),  // %
            Constraint::Fill(1),    // bar
            Constraint::Length(15), // time
        ])
        .split(now_playing[2]);
        // Dynamic coloring based on pane focus status
        let artist_border_color = if self.focus == Focus::Artists {
            Color::Green
        } else {
            Color::Gray
        };
        let album_border_color = if self.focus == Focus::Albums {
            Color::Green
        } else {
            Color::Gray
        };
        let song_border_color = if self.focus == Focus::Songs {
            Color::Green
        } else {
            Color::Gray
        };
        let queue_border_color = if self.focus == Focus::Queue {
            Color::Green
        } else {
            Color::Gray
        };

        let artists: Vec<ListItem> = self
            .library
            .artists
            .iter()
            .map(|artist| ListItem::new(artist.to_string()))
            .collect();

        let artist_list = List::new(artists)
            .block(
                Block::bordered()
                    .title("Artists")
                    .border_style(Style::new().fg(artist_border_color)).border_type(Rounded),
            )
            .style(Style::new().white())
            .highlight_symbol(">>")
            .repeat_highlight_symbol(true)
            .highlight_style(Style::new().reversed())
            .direction(ListDirection::TopToBottom);

        StatefulWidget::render(artist_list, left_area[0], buf, &mut self.artist_state);

        let albums: Vec<ListItem> = self
            .library
            .albums_for_artist(
                &self.library.artists[self.artist_state.selected().expect("None")].clone(),
            )
            .iter()
            .map(|albums| ListItem::new(albums.to_string()))
            .collect();

        let album_list = List::new(albums)
            .block(
                Block::bordered()
                    .title("Albums")
                    .border_style(Style::new().fg(album_border_color)).border_type(Rounded),
            )
            .style(Style::new().white())
            .highlight_symbol(">>")
            .repeat_highlight_symbol(true)
            .highlight_style(Style::new().reversed())
            .direction(ListDirection::TopToBottom);

        StatefulWidget::render(album_list, center_area[0], buf, &mut self.album_state);

        let song_indexes: Vec<usize> = self.current_selected_song();

        let song_items: Vec<ListItem> = song_indexes
            .iter()
            .map(|&index| ListItem::new(format!("{} ({})", self.library.songs[index].title.clone(), format_time(self.library.songs[index].duration.unwrap()))))
            .collect();

        let song_list = List::new(song_items)
            .block(
                Block::bordered()
                    .title("Songs")
                    .border_style(Style::new().fg(song_border_color)).border_type(Rounded),
            )
            .style(Style::new().white())
            .highlight_symbol(">>")
            .repeat_highlight_symbol(true)
            .highlight_style(Style::new().reversed())
            .direction(ListDirection::TopToBottom);

        StatefulWidget::render(song_list, right_area[0], buf, &mut self.song_state);

        let queue_items: Vec<ListItem> = self
            .queue
            .iter()
            .map(|&index| ListItem::new(format!("{} ({}))", self.library.songs[index].title.as_str(), format_time(self.library.songs[index].duration.unwrap()))))
            .collect();

        let queue_list = List::new(queue_items)
            .block(Block::bordered().title("Queue").border_style(Style::new().fg(queue_border_color)).border_type(Rounded))
            .style(Style::new().white())
            .highlight_symbol(">>")
            .repeat_highlight_symbol(true)
            .highlight_style(Style::new())
            .direction(ListDirection::TopToBottom);

        StatefulWidget::render(queue_list, right_area[1], buf, &mut self.queue_state);

        if let Some(song) = self.current_song() {
            let duration = song.duration.unwrap_or(1.0);
            Paragraph::new(song.title.as_str())
                .alignment(Alignment::Center)
                .render(now_playing[0], buf);

            Paragraph::new(format!("› {}", song.artist))
                .alignment(Alignment::Center)
                .style(Style::new().fg(Color::DarkGray))
                .render(now_playing[1], buf);

            let percent = position / duration;

            let line_gauge = LineGauge::default()
                .filled_style(Style::new().green().add_modifier(Modifier::BOLD))
                .unfilled_style(
                    Style::new()
                        .gray()
                        .add_modifier(Modifier::BOLD | Modifier::DIM),
                )
                .label(format!("{:>3.0}%", percent * 100.0))
                .ratio(percent.clamp(0.0, 1.0))
                .filled_symbol(symbols::line::HORIZONTAL)
                .unfilled_symbol(symbols::line::HORIZONTAL);

            Widget::render(line_gauge, progress[2], buf);

            Paragraph::new(format!(
                " {} / {}",
                format_time(position),
                format_time(duration),
            ))
            .render(progress[3], buf);
        } else {
            Paragraph::new("Nothing is playing").render(now_playing[0], buf);

            let line_gauge = LineGauge::default()
                .ratio(0.0)
                .filled_symbol(symbols::line::HORIZONTAL)
                .unfilled_symbol(symbols::line::HORIZONTAL);

            Widget::render(line_gauge, progress[2], buf);
        }

        Paragraph::new(if self.player.is_paused() { "||" } else { ">" }).render(progress[0], buf);
    }
}

fn format_time(seconds: f64) -> String {
    let secs = seconds as u64;
    format!("{}:{:02}", secs / 60, secs % 60)
}
