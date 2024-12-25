use eyre::Result;
use futures::executor::block_on;
use ratatui::Frame;

use crate::app::dialog::{
    DialogButton, DialogCommand, InputDialogState, MessageDialogState, ModalDialog,
};
use crate::app::ui_state::{MetricLivedataWindow, UIState};
use crate::client::state::Sensors;
use crate::model::sensor::{Metric, Sensor, ValueType, ValueUnit};
use crate::model::SensorId;
use ratatui::layout::Constraint::Ratio;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::symbols;
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Block, BorderType, Borders, Chart, Clear, Dataset, GraphType, List, ListItem, Paragraph,
    Tabs, Wrap,
};
use tokio::io::split;
use tokio::sync::oneshot;

pub fn render_state(frame: &mut Frame, sensors: &Sensors, ui_state: &UIState) -> Result<()> {
    let app_area = frame.area();

    // TODO Fetch name and version from Cargo.toml
    let app_title = Line::from(format!("{} v{}", "SensorVision", "0.1.0").bold());
    let instructions = Line::from(vec![
        " <Sensor Action> ".into(),
        "<Key>".light_blue().bold(),
        " <Metric Action> ".into(),
        "<Shift+Key>".light_blue().bold(),
        "|".into(),
        " Next ".into(),
        "<Tab>".blue().bold(),
        " New ".into(),
        "<n>".green().bold(),
        " Edit ".into(),
        "<e>".green().bold(),
        " Delete ".into(),
        "<d>".red().bold(),
        " Push Value ".into(),
        "<Space> ".green().bold(),
        "|".into(),
        " Quit ".into(),
        "<q> ".yellow().bold(),
    ]);
    let app_pad = Block::bordered()
        .title(app_title.centered())
        .title_bottom(instructions.centered())
        .style(Style::default().fg(Color::Cyan).bg(Color::Black))
        .border_set(border::THICK);

    if sensors.is_empty() {
        // TODO render empty state
        return Ok(());
    }

    let sensor_tabs = Tabs::new(
        sensors
            .iter()
            .map(|(_, sensor)| sensor.name.clone())
            .collect::<Vec<_>>(),
    )
    .block(app_pad)
    .highlight_style(Style::default().yellow())
    .divider(symbols::DOT)
    .select(ui_state.current_sensor.map(|(i, _)| i));

    frame.render_widget(sensor_tabs, app_area);

    if let Some((current_sensor, _)) = ui_state.current_sensor {
        let (_, current_sensor) = sensors.iter().nth(current_sensor).unwrap();
        render_sensor(frame, current_sensor, ui_state)?;
    }

    render_dialog(frame, ui_state, app_area)?;

    Ok(())
}

fn render_dialog(frame: &mut Frame, ui_state: &UIState, area: Rect) -> Result<()> {
    let Some(modal_dialog) = &ui_state.modal_dialog else {
        return Ok(());
    };

    use ModalDialog::*;

    match modal_dialog {
        Input(handle) => {
            let (tx, rx) = oneshot::channel::<InputDialogState>();
            handle.send_command(DialogCommand::Snapshot { respond_to: tx });
            let dialog_state = block_on(async move { rx.await })?;
            dialog_state.render(frame, area);
        }
        Confirmation(handle) => {
            let (tx, rx) = oneshot::channel::<MessageDialogState>();
            handle.send_command(DialogCommand::Snapshot { respond_to: tx });
            let dialog_state = block_on(async move { rx.await })?;
            dialog_state.render(frame, area);
        }
    }

    Ok(())
}

fn render_sensor(frame: &mut Frame, sensor: &Sensor<Metric>, ui_state: &UIState) -> Result<()> {
    let sensor_area = {
        let vbox = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Fill(1),
                Constraint::Length(3),
            ])
            .split(frame.area());
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(2),
                Constraint::Fill(1),
                Constraint::Length(2),
            ])
            .split(vbox[1])[1]
    };

    let vbox_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Percentage(40)])
        .split(sensor_area);

    let title = Paragraph::new(
        Line::from(vec![
            Span::styled(
                format!(
                    "{} {}",
                    emojis::get_by_shortcode("signal_strength").unwrap(),
                    sensor.name
                ),
                Style::default().fg(Color::LightBlue).bold(),
            ),
            Span::styled(" | ", Style::default().fg(Color::White)),
            Span::styled(
                format!(
                    "{}️ {}",
                    emojis::get_by_shortcode("id").unwrap(),
                    sensor.sensor_id
                ),
                Style::default().fg(Color::LightCyan),
            ),
        ])
        .centered(),
    );
    frame.render_widget(title, vbox_layout[0]);

    let metrics_count = sensor.metrics.len();

    if metrics_count == 0 {
        // TODO Render Empty State
        return Ok(());
    }

    // layout! cannot do that :(
    let metrics_areas = Layout::default()
        .direction(Direction::Horizontal)
        // TODO Consider limiting metrics in a row
        .constraints(vec![Ratio(1, metrics_count as u32); metrics_count])
        .split(vbox_layout[1]);
    for i in 0..metrics_count {
        let metric = &sensor.metrics[i];
        render_metric(frame, metrics_areas[i], ui_state, metric, sensor.sensor_id)?;
    }

    Ok(())
}

fn render_metric(
    frame: &mut Frame,
    area: Rect,
    ui_state: &UIState,
    metric: &Metric,
    sensor_id: SensorId,
) -> Result<()> {
    let mut list_items = Vec::<ListItem>::new();
    let id: String;
    let name: String;

    match metric {
        Metric::Predefined {
            metric_id,
            name: metric_name,
            value_unit,
        } => {
            id = metric_id.to_string();
            name = metric_name.clone();
            list_items.push(ListItem::new(Line::from(Span::styled(
                value_unit.emojified(),
                Style::default().fg(Color::LightGreen),
            ))));
        }

        Metric::Custom {
            metric_id,
            name: metric_name,
            value_type,
            value_annotation,
        } => {
            id = metric_id.to_string();
            name = metric_name.clone();
            list_items.push(ListItem::new(Line::from(Span::styled(
                value_type.emojified(),
                Style::default().fg(Color::LightGreen),
            ))));
            list_items.push(ListItem::new(Line::from(Span::styled(
                format!(
                    "{} {}",
                    emojis::get_by_shortcode("writing_hand").unwrap(),
                    value_annotation
                ),
                Style::default().fg(Color::LightBlue),
            ))));
        }
    }

    list_items.insert(
        0,
        ListItem::new(Line::from(Span::styled(
            format!("{} {:.20}", emojis::get_by_shortcode("id").unwrap(), id),
            Style::default().fg(Color::LightBlue),
        ))),
    );

    let content_area = centered_rect(96, 92, area);
    let vbox_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Percentage(100)])
        .split(content_area);
    let metric_props_list = List::new(list_items);
    frame.render_widget(metric_props_list, vbox_layout[0]);

    let livedata_key = (sensor_id, *metric.metric_id());

    let mut metric_props_block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(Span::styled(name, Style::default().fg(Color::LightGreen))).centered())
        .border_type(BorderType::Rounded);
    if ui_state
        .current_metric
        .is_some_and(|(_, metric_id)| *metric.metric_id() == metric_id)
    {
        metric_props_block =
            metric_props_block.border_style(Style::default().fg(Color::LightMagenta));
    }
    frame.render_widget(metric_props_block, area);

    if let Some(livedata) = ui_state.livedata.get(&livedata_key) {
        match metric {
            Metric::Predefined { value_unit, .. } => {
                let annotation = format!("{:?}", value_unit);
                frame.render_widget(
                    numeric_livedata_chart(&livedata, &annotation),
                    vbox_layout[1],
                );
            }
            Metric::Custom {
                value_type,
                value_annotation,
                ..
            } => {
                match value_type {
                    ValueType::Double | ValueType::Integer | ValueType::Boolean => {
                        let annotation = format!("{:?}", value_annotation);
                        frame.render_widget(
                            numeric_livedata_chart(&livedata, &annotation),
                            vbox_layout[1],
                        );
                    }
                    // TODO Render string chart
                    _ => {}
                };
            }
        };
    } else {
        let no_data = Line::from("NO DATA").magenta().bold().centered();
        frame.render_widget(no_data, vbox_layout[1]);
    }

    Ok(())
}

fn numeric_livedata_chart<'a>(
    livedata_window: &'a MetricLivedataWindow,
    annotation: &'a str,
) -> Chart<'a> {
    let datasets = vec![
        Dataset::default()
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().light_green())
            .data(&livedata_window.data),
        Dataset::default()
            .marker(symbols::Marker::Dot)
            .graph_type(GraphType::Scatter)
            .style(Style::default().red())
            .data(&livedata_window.data),
    ];

    let x_axis = Axis::default()
        .style(Style::default().white())
        .bounds([livedata_window.min_timestamp, livedata_window.max_timestamp])
        .labels([
            livedata_window.min_timestamp_str.clone(),
            livedata_window.max_timestamp_str.clone(),
        ]);

    let y_axis = Axis::default()
        .title(annotation.red())
        .style(Style::default().white())
        .bounds([livedata_window.min_value, livedata_window.max_value])
        .labels([
            livedata_window.min_value_str.clone(),
            livedata_window.max_value_str.clone(),
        ]);

    let chart_block = Block::default()
        .borders(Borders::ALL)
        .title(
            Line::from(Span::styled(
                "Livedata",
                Style::default().fg(Color::Red).bg(Color::Black),
            ))
            .centered(),
        )
        .border_type(BorderType::Thick);

    Chart::new(datasets)
        .block(chart_block)
        .x_axis(x_axis)
        .y_axis(y_axis)
}

impl MessageDialogState {
    fn render(&self, frame: &mut Frame, area: Rect) {
        let area = centered_rect(30, 20, area);

        let instructions = Line::from(vec![
            " Select Button ".into(),
            "<Tab>".blue().bold(),
            " Press ".into(),
            "<Enter>".blue().bold(),
            " Close ".into(),
            "<Esc> ".blue().bold(),
        ]);
        let pad = Block::bordered()
            .title(Line::from(self.title.as_str()).centered())
            .title_bottom(instructions.centered())
            .bg(Color::Indexed(172));

        let content_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                // Header
                Constraint::Length(1),
                // Text area
                Constraint::Fill(1),
                // Buttons area { [   OK   ]_[  CANCEL  ] }
                Constraint::Length(1),
                // Footer
                Constraint::Length(1),
            ])
            .split(area);

        let buttons_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                // { [   OK   ]_[  CANCEL  ] }
                Constraint::Min(1),
                Constraint::Length(10),
                Constraint::Length(1),
                Constraint::Length(10),
                Constraint::Min(1),
            ])
            .split(content_layout[2]);

        let text = Paragraph::new(self.text.as_str())
            .centered()
            .wrap(Wrap { trim: false });

        frame.render_widget(Clear, area);
        frame.render_widget(pad, area);
        frame.render_widget(text, content_layout[1]);

        DialogButton::Ok.render(frame, buttons_layout[1], self.focused_button);
        DialogButton::Cancel.render(frame, buttons_layout[3], self.focused_button);
    }
}

impl InputDialogState {
    fn render(&self, frame: &mut Frame, area: Rect) {
        let area = centered_rect(30, 20, area);

        let instructions = Line::from(vec![
            " Select Button ".into(),
            "<Tab>".blue().bold(),
            " Press ".into(),
            "<Enter>".blue().bold(),
            " Close ".into(),
            "<Esc> ".blue().bold(),
        ]);

        let pad = Block::bordered()
            .title(Line::from(self.title.clone()).centered())
            .title_bottom(instructions.centered())
            .bg(Color::Indexed(172));

        let content_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                // 0 Header
                Constraint::Length(1),
                // 1 Text area
                Constraint::Fill(1),
                // 2 Text input { Label [<input>          ] }
                Constraint::Length(1),
                // 3 Buttons area { [   OK   ]_[  CANCEL  ] }
                Constraint::Length(1),
                // 4 Footer
                Constraint::Length(1),
            ])
            .split(area);

        let input_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                // { Label [<input>          ] }
                Constraint::Length(1),
                Constraint::Length(10),
                Constraint::Percentage(80),
                Constraint::Min(1),
            ])
            .split(content_layout[2]);

        let buttons_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                // { [   OK   ]_[  CANCEL  ] }
                Constraint::Min(1),
                Constraint::Length(10),
                Constraint::Length(1),
                Constraint::Length(10),
                Constraint::Min(1),
            ])
            .split(content_layout[3]);

        let text = Paragraph::new(self.text.as_str())
            .centered()
            .wrap(Wrap { trim: false });

        let label = Line::from(self.label.as_str());
        let text_input_pad = Block::new().bg(Color::Indexed(75));
        let text_input = Line::from(
            self.text_input
                .as_ref()
                .map(|s| s.as_str())
                .unwrap_or_else(|| "<input>"),
        );

        frame.render_widget(Clear, area);
        frame.render_widget(pad, area);
        frame.render_widget(text, content_layout[1]);
        frame.render_widget(label, input_layout[1]);
        frame.render_widget(text_input_pad, input_layout[2]);
        frame.render_widget(text_input, input_layout[2]);

        DialogButton::Ok.render(frame, buttons_layout[1], self.focused_button);
        DialogButton::Cancel.render(frame, buttons_layout[3], self.focused_button);
    }
}

impl DialogButton {
    fn render(&self, frame: &mut Frame, area: Rect, focused: Option<DialogButton>) {
        let text = match self {
            Self::Ok => "OK",
            Self::Cancel => "CANCEL",
        };

        let mut button_block = Block::default()
            .borders(Borders::LEFT | Borders::RIGHT)
            .border_type(BorderType::Rounded)
            .style(Style::new().bg(Color::Gray));

        if let Some(focused) = focused {
            if focused == *self {
                button_block = button_block.style(Style::new().bg(Color::LightBlue));
            }
        }

        let button = Paragraph::new(text).block(button_block);

        frame.render_widget(button, area);
    }
}

trait Emojified {
    fn emojified(&self) -> String;
}

impl Emojified for ValueUnit {
    fn emojified(&self) -> String {
        let shortcode = match self {
            ValueUnit::Ampere
            | ValueUnit::Farad
            | ValueUnit::Ohm
            | ValueUnit::Volt
            | ValueUnit::Watt => "zap",
            ValueUnit::Bit => "keycap_ten",
            ValueUnit::Candela => "bulb",
            ValueUnit::Celsius => "thermometer",
            ValueUnit::Decibel => "loud_sound",
            ValueUnit::Hertz => "signal_strength",
            ValueUnit::Joule => "battery",
            ValueUnit::Kilogram => "scales",
            ValueUnit::Latitude | ValueUnit::Longitude => "world_map",
            ValueUnit::Meter => "straight_ruler",
            ValueUnit::MetersPerSecond => "bullettrain_side",
            ValueUnit::MetersPerSquareSecond => "rocket",
            ValueUnit::Mole => "test_tube",
            ValueUnit::Newton => "apple",
            ValueUnit::Pascal => "tornado",
            ValueUnit::Percent => "100",
            ValueUnit::Radian | ValueUnit::SquareMetre => "triangular_ruler",
            ValueUnit::Second => "watch",
        };
        format!(
            "{} {:?}",
            emojis::get_by_shortcode(shortcode).unwrap(),
            self
        )
    }
}

impl Emojified for ValueType {
    fn emojified(&self) -> String {
        let shortcode = match self {
            ValueType::Boolean => "keycap_ten",
            ValueType::Integer => "1234",
            ValueType::Double => "heavy_division_sign",
            ValueType::String => "pencil",
        };
        format!(
            "{} {:?}",
            emojis::get_by_shortcode(shortcode).unwrap(),
            self
        )
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    // Then cut the middle vertical piece into three width-wise pieces
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1] // Return the middle chunk
}
