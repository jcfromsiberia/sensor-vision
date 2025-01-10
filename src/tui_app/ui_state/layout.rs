use eyre::Result;

use taffy::prelude::*;

use ratatui::layout::Rect;

pub fn metric_dyn_layout(
    metric_count: usize,
    area: Rect,
    min_width: u16,
    min_height: u16,
) -> Result<Vec<Rect>> {
    let mut taffy: TaffyTree<()> = TaffyTree::new();

    let num_columns = (area.width / min_width) as usize;
    let grid_container = taffy.new_with_children(
        Style {
            display: Display::Grid,
            grid_template_columns: vec![fr(1.0); num_columns],
            gap: Size {
                width: length(1.0),
                height: length(0.0),
            },
            ..Default::default()
        },
        &[],
    )?;

    for _ in 0..metric_count {
        let metric_node = taffy.new_leaf(Style {
            min_size: Size {
                width: length(min_width as f32),
                height: length(min_height as f32),
            },
            size: Size {
                width: auto(),
                height: auto(),
            },
            ..Default::default()
        })?;
        taffy.add_child(grid_container, metric_node)?;
    }

    taffy.compute_layout(grid_container, Size::MAX_CONTENT)?;

    Ok(taffy
        .children(grid_container)?
        .iter()
        .map(|node_id| {
            let metric_area = taffy.layout(*node_id).unwrap();
            Rect {
                x: metric_area.location.x as u16 + area.x,
                y: metric_area.location.y as u16 + area.y,
                width: metric_area.size.width as u16,
                height: metric_area.size.height as u16,
            }
        })
        .collect())
}
