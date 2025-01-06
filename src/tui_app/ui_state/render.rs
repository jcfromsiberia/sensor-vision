use actix::{AsyncContext, Handler, Message, WrapFuture};

use ratatui::layout::Constraint::Ratio;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::symbols;
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Block, BorderType, Borders, Chart, Dataset, GraphType, List, ListItem, Paragraph,
    Tabs,
};
use ratatui::Frame;

use crate::client::state::Sensors;
use crate::model::sensor::{Metric, Sensor, ValueType, ValueUnit};
use crate::model::SensorId;
use crate::tui_app::dialog::*;
use crate::tui_app::dialog::render::Renderable;
use crate::tui_app::ui_state::{MetricLivedataWindow, UIState};

use crate::tui_app::tui::SharedTui;

#[derive(Message)]
#[rtype(result = "()")]
pub struct Render {
    pub tui: SharedTui,
    pub sensors: Sensors,
}

impl Handler<Render> for UIState {
    type Result = ();

    fn handle(&mut self, Render { tui, sensors }: Render, ctx: &mut Self::Context) -> Self::Result {
        let ui_state = self.clone();

        ctx.spawn(
            async move {
                let dialog_to_render: Option<Box<dyn Renderable>> = match &ui_state.modal_dialog {
                    Some(ModalDialog::Confirmation(dialog)) => {
                        if let Ok(dialog_state) =
                            dialog.send(StateSnapshot::<ConfirmationDialogState>::default()).await
                        {
                            Some(Box::new(dialog_state))
                        } else {
                            None
                        }
                    },
                    Some(ModalDialog::Input(dialog)) => {
                        if let Ok(dialog_state) =
                            dialog.send(StateSnapshot::<InputDialogState>::default()).await
                        {
                            Some(Box::new(dialog_state))
                        } else {
                            None
                        }
                    },
                    None => None,
                };

                let _ = tui.lock().await.terminal.draw(move |frame| {
                    render_state(frame, &sensors, &ui_state);
                    if let Some(dialog) = dialog_to_render {
                        dialog.render(frame);
                    }
                });
            }
            .into_actor(self),
        );
    }
}

fn render_state(frame: &mut Frame, sensors: &Sensors, ui_state: &UIState) {
    let app_area = frame.area();

    // TODO Fetch name and version from Cargo.toml
    let app_title = Line::from(format!("{} v{}", "SensorVision", "0.1.0").bold());
    let instructions = Line::from(vec![
        " <Sensor Action> ".into(),
        "<Key>".light_blue().bold(),
        " <Metric Action> ".into(),
        "<Shift+Key> ".light_blue().bold(),
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
        let no_sensors = Paragraph::new(
            Line::from("Current connector has no sensors"))
            .red()
            .centered()
            .block(app_pad);
        frame.render_widget(no_sensors, app_area);
        return;
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
        render_sensor(frame, current_sensor, ui_state);
    }
}

fn render_sensor(frame: &mut Frame, sensor: &Sensor<Metric>, ui_state: &UIState) {
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
        return;
    }

    // layout! cannot do that :(
    let metrics_areas = Layout::default()
        .direction(Direction::Horizontal)
        // TODO Consider limiting metrics in a row
        .constraints(vec![Ratio(1, metrics_count as u32); metrics_count])
        .split(vbox_layout[1]);
    for i in 0..metrics_count {
        let metric = &sensor.metrics[i];
        render_metric(frame, metrics_areas[i], ui_state, metric, sensor.sensor_id);
    }
}

fn render_metric(
    frame: &mut Frame,
    area: Rect,
    ui_state: &UIState,
    metric: &Metric,
    sensor_id: SensorId,
) {
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

pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
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
