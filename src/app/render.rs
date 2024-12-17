use crate::app::AppStateWrapper;
use crate::model::sensor::{Metric, Sensor, ValueType, ValueUnit};
use crate::model::ToMqttId;
use crossterm::event::{Event, KeyCode};
use ratatui::layout::Constraint::Ratio;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::symbols;
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Block, BorderType, Borders, Chart, Dataset, GraphType, List, ListItem, Paragraph
    , Tabs,
};
use uuid::Uuid;
use widgetui::{constraint, layout, Chunks, Events, ResMut, WidgetFrame, WidgetResult};
use crate::app::ui_state::{MetricLivedataWindow, UIState};

struct AppArea;
struct SensorArea;
pub fn render_sensor_vision(
    mut frame: ResMut<WidgetFrame>,
    mut state_wrapper: ResMut<AppStateWrapper>,
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
    let instructions = Line::from(vec![
        " Next Sensor ".into(),
        "<Tab>".blue().bold(),
        " Quit ".into(),
        "<q> ".blue().bold(),
    ]);
    let app_pad = Block::bordered()
        .title(app_title.centered())
        .title_bottom(instructions.centered())
        .border_set(border::THICK);

    let (sensors_snapshot, ui_state_snapshot) = {
        let app_state = state_wrapper.app_state.read().unwrap();
        let sensors = app_state.state.read().unwrap().sensors.clone();
        let ui_state = app_state.ui_state.clone();
        (sensors, ui_state)
    };

    if sensors_snapshot.is_empty() {
        // TODO render empty state
        return Ok(());
    }

    let sensor_tabs = Tabs::new(
        sensors_snapshot
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
        let (_, current_sensor) = sensors_snapshot.iter().nth(current_sensor).unwrap();
        render_sensor(&mut frame, &chunks, &ui_state_snapshot, current_sensor)?;
    }

    // render dialogs

    handle_events(&mut events, &mut state_wrapper);

    Ok(())
}

fn handle_events(events: &mut ResMut<Events>, state_wrapper: &mut ResMut<AppStateWrapper>) {
    if let Some(event) = &events.event {
        match event {
            Event::Key(key_event) if key_event.code == KeyCode::Char('q') => {
                events.register_exit();
            },
            Event::Key(key_event) if key_event.code == KeyCode::Tab => {
                let mut app_state = state_wrapper.app_state.write().unwrap();
                app_state.next_sensor();
            },
            _ => {}
        }
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

fn render_sensor(
    frame: &mut ResMut<WidgetFrame>,
    chunks: &ResMut<Chunks>,
    ui_state: &UIState,
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
        // TODO Consider limiting metrics in a row
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
    ui_state: &UIState,
    metric: &Metric,
    sensor_id: &Uuid,
) -> WidgetResult {
    let mut list_items = Vec::<ListItem>::new();
    let id: String;
    let name: String;

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

    {
        let mut x = MetricLivedataWindow::default();
        x.push_data(1734280090215, 55.0);
        x.push_data(1724280090216, 56.9);
        x.push_data(1734280090218, 99.0);
        x.push_data(1734280090213, 25.0);
        render_numeric_livedata(frame, &vbox[1][0], &x, "Percent")?;
    }

    match metric {
        Metric::Predefined { value_unit, .. } => {
            if let Some(data) = ui_state.livedata.get(&key) {
                let annotation = format!("{:?}", value_unit);
                render_numeric_livedata(frame, &vbox[1][0], &data, &annotation)?;
            }
        }
        Metric::Custom {
            value_type,
            value_annotation,
            ..
        } => {
            match value_type {
                ValueType::Double | ValueType::Integer | ValueType::Integer => {
                    if let Some(data) = ui_state.livedata.get(&key) {
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
    livedata_window: &MetricLivedataWindow,
    annotation: &str,
) -> WidgetResult {
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
        .labels([livedata_window.min_timestamp_str.clone(), livedata_window.max_timestamp_str.clone()]);

    let y_axis = Axis::default()
        .title(annotation.red())
        .style(Style::default().white())
        .bounds([livedata_window.min_value, livedata_window.max_value])
        .labels([livedata_window.min_value_str.clone(), livedata_window.max_value_str.clone()]);

    let chart_block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(Span::styled("Livedata", Style::default().fg(Color::Red))).centered())
        .border_type(BorderType::QuadrantOutside);

    let chart = Chart::new(datasets)
        .block(chart_block)
        .x_axis(x_axis)
        .y_axis(y_axis);

    frame.render_widget(chart, centered_area(96, 96, *area));
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
