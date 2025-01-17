use actix::{AsyncContext, Handler, Message, WrapFuture};

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::symbols;
use ratatui::symbols::border;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Axis, Block, BorderType, Borders, Chart, Dataset, GraphType, List, ListItem, Paragraph, Tabs,
    Wrap,
};
use ratatui::Frame;

use crate::client::state::Sensors;
use crate::model::sensor::{Metric, Sensor, ValueType};
use crate::model::SensorId;
use crate::tui_app::dialog::render::Renderable;
use crate::tui_app::dialog::*;
use crate::tui_app::ui_state::layout::metric_dyn_layout;
use crate::tui_app::ui_state::{MetricLivedataWindow, UIState};

use crate::tui_app::theme::*;
use crate::tui_app::tui::SharedTui;
use crate::tui_app::utils;

use crate::tui_app::theme::Emojified;
use UIElement::*;

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
                        if let Ok(dialog_state) = dialog
                            .send(StateSnapshot::<ConfirmationDialogState>::default())
                            .await
                        {
                            Some(Box::new(dialog_state))
                        } else {
                            None
                        }
                    }
                    Some(ModalDialog::Input(dialog)) => {
                        if let Ok(dialog_state) = dialog
                            .send(StateSnapshot::<InputDialogState>::default())
                            .await
                        {
                            Some(Box::new(dialog_state))
                        } else {
                            None
                        }
                    }
                    Some(ModalDialog::Metric(dialog)) => {
                        if let Ok(dialog_state) = dialog
                            .send(StateSnapshot::<MetricDialogState>::default())
                            .await
                        {
                            Some(Box::new(dialog_state))
                        } else {
                            None
                        }
                    }
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
        " <Sensor Action> ".themed(InstructionsText),
        "<Key>".themed(InstructionsActionText).bold(),
        " <Metric Action> ".themed(InstructionsText),
        "<⇧ + Key> ".themed(InstructionsActionText).bold(),
        "|".themed(InstructionsText),
        " Next ".themed(InstructionsText),
        "↹ ".themed(InstructionsActionText).bold(),
        " New ".themed(InstructionsText),
        "n".themed(InstructionsActionText).bold(),
        " Edit ".themed(InstructionsText),
        "e".themed(InstructionsActionText).bold(),
        " Delete ".themed(InstructionsText),
        "d".themed(InstructionsActionText).bold(),
        " Push Value ".themed(InstructionsText),
        "␣ ".themed(InstructionsActionText).bold(),
        "|".themed(InstructionsText),
        " Quit ".themed(InstructionsText),
        "q ".themed(InstructionsActionText).bold(),
    ]);
    let app_pad = Block::bordered()
        .title(app_title.centered())
        .title_bottom(instructions.centered())
        .style(Style::default().themed(AppPad))
        .border_set(border::THICK);

    if sensors.is_empty() {
        let no_sensors = Paragraph::new(Line::from("Current connector has no sensors"))
            .themed(NoSensors)
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
    .highlight_style(Style::default().themed(SelectedSensorTab))
    .divider(symbols::DOT)
    .select(ui_state.current_sensor.map(|(i, _)| i));

    frame.render_widget(sensor_tabs, app_area);

    if let Some((current_sensor, _)) = ui_state.current_sensor {
        let (_, current_sensor) = sensors.iter().nth(current_sensor).unwrap();
        render_sensor(frame, current_sensor, ui_state);
    }
}

fn render_sensor(frame: &mut Frame, sensor: &Sensor<Metric>, ui_state: &UIState) {
    let metrics_count = sensor.metrics.len();

    // Cut boundaries and Tabs
    let area = {
        let vbox = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Fill(1),
                Constraint::Length(1),
            ])
            .split(frame.area());
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(1),
                Constraint::Fill(1),
                Constraint::Length(1),
            ])
            .split(vbox[1])[1]
    };

    let vbox = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(ui_state.errors.len() as u16),
        ])
        .split(area);

    if !ui_state.errors.is_empty() {
        let error_log = ui_state.errors.iter().fold(String::default(), |a, b| a + "\n" + b).trim().to_string();
        let errors_log = Paragraph::new(Text::from(error_log))
            .themed(ErrorLog)
            .wrap(Wrap { trim: true });
        frame.render_widget(errors_log, vbox[1]);
    }

    let sensor_area = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(2),
        ])
        .split(vbox[0])[1];

    let vbox_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(13)])
        .split(sensor_area);

    if metrics_count == 0 {
        let no_metrics = Paragraph::new(Line::from("Current sensor has no metrics"))
            .themed(NoMetrics)
            .centered();
        frame.render_widget(no_metrics, vbox_layout[0]);
        return;
    }

    let title = Paragraph::new(
        Line::from(vec![
            Span::styled(
                format!(
                    "{} {}",
                    emojis::get_by_shortcode("signal_strength").unwrap(),
                    sensor.name
                ),
                Style::default().themed(SensorName).bold(),
            ),
            Span::styled(" | ", Style::default().themed(InstructionsText)),
            Span::styled(
                format!(
                    "{}️ {}",
                    emojis::get_by_shortcode("id").unwrap(),
                    sensor.sensor_id
                ),
                Style::default().themed(SensorId),
            ),
        ])
        .centered(),
    );
    frame.render_widget(title, vbox_layout[0]);

    if let Ok(metric_areas) = metric_dyn_layout(metrics_count, vbox_layout[1], 50, 20) {
        for i in 0..metrics_count {
            let metric = &sensor.metrics[i];
            render_metric(frame, metric_areas[i], ui_state, metric, sensor.sensor_id);
        }
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
                Style::default().themed(MetricValueUnit),
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
                Style::default().themed(MetricValueType),
            ))));
            list_items.push(ListItem::new(Line::from(Span::styled(
                format!(
                    "{} {}",
                    emojis::get_by_shortcode("writing_hand").unwrap(),
                    value_annotation
                ),
                Style::default().themed(MetricValueAnnotation),
            ))));
        }
    }

    list_items.insert(
        0,
        ListItem::new(Line::from(Span::styled(
            format!("{} {:.20}", emojis::get_by_shortcode("id").unwrap(), id),
            Style::default().themed(MetricId),
        ))),
    );

    let content_area = utils::centered_rect_abs(area.width - 2, area.height - 2, area);
    let vbox_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Percentage(100)])
        .split(content_area);
    let metric_props_list = List::new(list_items);
    frame.render_widget(metric_props_list, vbox_layout[0]);

    let livedata_key = (sensor_id, *metric.metric_id());

    let mut metric_props_block = Block::default()
        .borders(Borders::ALL)
        .themed(MetricPropsBlock)
        .title(Line::from(Span::styled(name, Style::default().themed(MetricName))).centered())
        .border_type(BorderType::Rounded);
    if ui_state
        .current_metric
        .is_some_and(|(_, metric_id)| *metric.metric_id() == metric_id)
    {
        metric_props_block =
            metric_props_block.border_style(Style::default().themed(MetricPropsBlockSelected));
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
        let no_data = Line::from("NO DATA").themed(MetricNoData).bold().centered();
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
            .themed(LivedataLine)
            .data(&livedata_window.data),
        Dataset::default()
            .marker(symbols::Marker::Dot)
            .graph_type(GraphType::Scatter)
            .themed(LivedataScatter)
            .data(&livedata_window.data),
    ];

    let x_axis = Axis::default()
        .themed(InstructionsText)
        .bounds([livedata_window.min_timestamp, livedata_window.max_timestamp])
        .labels([
            livedata_window.min_timestamp_str.clone(),
            livedata_window.max_timestamp_str.clone(),
        ]);

    let y_axis = Axis::default()
        .title(annotation.themed(InstructionsText))
        .themed(InstructionsText)
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
                Style::default().themed(InstructionsText),
            ))
            .centered(),
        )
        .border_type(BorderType::Thick);

    Chart::new(datasets)
        .block(chart_block)
        .x_axis(x_axis)
        .y_axis(y_axis)
        .themed(LivedataChart)
}
