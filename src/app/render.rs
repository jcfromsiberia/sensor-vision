use crate::app::{AppSharedUIState, AppState};
use crate::model::sensor::{Metric, Sensor, ValueType, ValueUnit};
use crate::model::ToMqttId;
use crossterm::event::KeyCode;
use ratatui::layout::Constraint::Ratio;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::symbols;
use ratatui::symbols::border;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, List, ListItem, Paragraph, RenderDirection, Sparkline,
    SparklineBar, Tabs, Widget,
};
use std::collections::VecDeque;
use uuid::Uuid;
use widgetui::{constraint, layout, Chunks, Events, ResMut, WidgetFrame, WidgetResult};

struct AppArea;
struct SensorArea;
pub fn render_sensor_vision(
    mut frame: ResMut<WidgetFrame>,
    mut app_state: ResMut<AppState>,
    mut events: ResMut<Events>,
    mut chunks: ResMut<Chunks>,
) -> WidgetResult {
    chunks.register_chunk::<AppArea>(frame.size());
    let app_area = chunks.get_chunk::<AppArea>()?;

    let sensor_area = layout! {
        frame.size(),
        (#2),
        (%100) => {#2, %100, #2},
        (#3)
    }[1][1];
    chunks.register_chunk::<SensorArea>(sensor_area);

    // TODO Fetch name and version from Cargo.toml
    let app_title = Line::from(format!("{} v{}", "SensorVision", "0.1.0").bold());
    let app_pad = Block::bordered()
        .title(app_title.centered())
        .border_set(border::THICK);

    let mut ui_state_snapshot = app_state.ui_state.read().unwrap().clone();
    ui_state_snapshot.current_sensor_index = Some(0);

    // Read lock
    let state = &app_state.state.read().unwrap();

    if state.sensors.is_empty() {
        // TODO render empty state
    }

    let sensor_tabs = Tabs::new(
        state
            .sensors
            .iter()
            .map(|(_, sensor)| sensor.name.clone())
            .collect::<Vec<_>>(),
    )
    .block(app_pad)
    .highlight_style(Style::default().yellow())
    .divider(symbols::DOT)
    .select(ui_state_snapshot.current_sensor_index);

    frame.render_widget(sensor_tabs, app_area);

    if let Some(current_sensor) = ui_state_snapshot.current_sensor_index {
        let (_, current_sensor) = state.sensors.iter().nth(current_sensor).unwrap();
        render_sensor(&mut frame, &chunks, &ui_state_snapshot, current_sensor)?;
    }

    if events.key(KeyCode::Char('q')) {
        events.register_exit();
    }

    Ok(())
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

fn render_sensor(
    frame: &mut ResMut<WidgetFrame>,
    chunks: &ResMut<Chunks>,
    ui_state: &AppSharedUIState,
    sensor: &Sensor<Metric>,
) -> WidgetResult {
    let sensor_area = chunks.get_chunk::<SensorArea>()?;
    let vbox = &layout! {
        sensor_area,
        (#1),
        (%50)
    };
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
                    sensor.sensor_id.to_mqtt()
                ),
                Style::default().fg(Color::LightCyan),
            ),
        ])
        .centered(),
    );
    frame.render_widget(title, vbox[0][0]);

    let metrics_count = sensor.metrics.len();

    if metrics_count == 0 {
        // TODO Render Empty State
        return Ok(());
    }

    // layout! cannot do that :(
    let metrics_areas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Ratio(1, metrics_count as u32); metrics_count])
        .split(vbox[1][0]);
    for i in 0..metrics_count {
        let metric = &sensor.metrics[i];
        render_metric(
            frame,
            &metrics_areas[i],
            &ui_state,
            metric,
            &sensor.sensor_id,
        )?;
    }

    Ok(())
}

fn render_metric(
    frame: &mut ResMut<WidgetFrame>,
    area: &Rect,
    ui_state: &AppSharedUIState,
    metric: &Metric,
    sensor_id: &Uuid,
) -> WidgetResult {
    let mut list_items = Vec::<ListItem>::new();
    let mut id: String;
    let mut name: String;

    match metric {
        Metric::Predefined {
            metric_id,
            name: metric_name,
            value_unit,
        } => {
            id = metric_id.to_mqtt();
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
            id = metric_id.to_mqtt();
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

    let metric_props_block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(Span::styled(name, Style::default().fg(Color::LightGreen))).centered())
        .border_type(BorderType::Rounded);

    let vbox = layout! {
        *area,
        (#6),
        (%100)
    };

    let metric_props_list = List::new(list_items).block(metric_props_block);

    frame.render_widget(metric_props_list, vbox[0][0]);

    let key = (*sensor_id, *metric.metric_id());

    match metric {
        Metric::Predefined { value_unit, .. } => {
            if let Some(data) = ui_state.livedata_double.get(&key) {
                let annotation = format!("{:?}", value_unit);
                let data = data.iter().map(|v| *v as u64).collect::<Vec<_>>();
                render_numeric_livedata(frame, &vbox[1][0], &data, &annotation)?;
            }
        }
        Metric::Custom {
            value_type,
            value_annotation,
            ..
        } => {
            match value_type {
                ValueType::Double => {
                    if let Some(data) = ui_state.livedata_double.get(&key) {
                        let data = data.iter().map(|v| *v as u64).collect::<Vec<_>>();
                        render_numeric_livedata(frame, &vbox[1][0], &data, &value_annotation)?;
                    }
                }
                ValueType::Integer => {
                    if let Some(data) = ui_state.livedata_integer.get(&key) {
                        let data = data.iter().map(|v| *v as u64).collect::<Vec<_>>();
                        render_numeric_livedata(frame, &vbox[1][0], &data, &value_annotation)?;
                    }
                }
                ValueType::Boolean => {
                    if let Some(data) = ui_state.livedata_boolean.get(&key) {
                        let data = data.iter().map(|v| *v as u64).collect::<Vec<_>>();
                        render_numeric_livedata(frame, &vbox[1][0], &data, &value_annotation)?;
                    }
                }
                // TODO Render string chart
                _ => {}
            };
        }
    }

    Ok(())
}

fn render_numeric_livedata(
    frame: &mut ResMut<WidgetFrame>,
    area: &Rect,
    data: &Vec<u64>,
    annotation: &str,
) -> WidgetResult {
    let values_chart = Sparkline::default()
        .block(Block::bordered().title(Line::from(annotation).centered()))
        .data(data)
        .direction(RenderDirection::LeftToRight)
        .style(Style::default().red().on_white())
        .absent_value_style(Style::default().fg(Color::Red))
        .absent_value_symbol(symbols::shade::FULL);

    frame.render_widget(values_chart, *area);
    Ok(())
}

fn centered_area(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let x_crop_percent = (100 - percent_x) / 2;
    let y_crop_percent = (100 - percent_y) / 2;
    (layout! {
        area,
        (%y_crop_percent),
        (%percent_y) => { %x_crop_percent, %percent_x, %x_crop_percent },
        (%y_crop_percent)
    })[1][1]
}
