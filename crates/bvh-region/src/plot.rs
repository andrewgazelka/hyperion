use fastrand::u8;
use geometry::aabb::Aabb;
use plotters::{
    chart::{ChartBuilder, ChartContext},
    coord::types::RangedCoordf32,
    drawing::IntoDrawingArea,
    element::Rectangle,
    prelude::Cartesian2d,
    style::{BLACK, Color, RED, RGBColor, ShapeStyle},
};
use plotters_bitmap::BitMapBackend;
use tracing::debug;

use crate::{Bvh, HasAabb, Node};

impl<T: HasAabb + Copy> Bvh<T> {
    pub fn plot(&self, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
        let root_area = BitMapBackend::new(filename, (1000, 1000)).into_drawing_area();
        root_area.fill(&BLACK)?;

        let node = self.root();

        let Node::Internal(internal) = node else {
            panic!("Root node is not internal")
        };

        debug!(
            "min x = {}, min y = {}, max x = {}, max y = {}",
            internal.aabb.min.x, internal.aabb.min.y, internal.aabb.max.x, internal.aabb.max.y
        );

        let aabb = internal.aabb;

        let mut chart = ChartBuilder::on(&root_area)
            .build_cartesian_2d(aabb.min.x..aabb.max.x, aabb.min.y..aabb.max.y)?;

        // chart.configure_mesh().gr.draw()?;
        self.draw_node(&mut chart, Some(node))?;

        Ok(())
    }

    fn draw_node(
        &self,
        chart: &mut ChartContext<
            '_,
            BitMapBackend<'_>,
            Cartesian2d<RangedCoordf32, RangedCoordf32>,
        >,
        node: Option<Node<'_, T>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(node) = node {
            match node {
                Node::Internal(internal) => {
                    let style = RED.mix(0.15).stroke_width(1);
                    self.draw_aabb(style, chart, &internal.aabb)?;

                    for elem in internal.children(self) {
                        self.draw_node(chart, Some(elem))?;
                    }
                }
                Node::Leaf(leaf) => {
                    let color = random_color();
                    let style = color.mix(0.35).filled().stroke_width(1);
                    for elem in leaf.iter() {
                        self.draw_aabb(style, chart, &elem.aabb())?;
                    }
                }
            }
        }
        Ok(())
    }

    fn draw_aabb(
        &self,
        style: ShapeStyle,
        chart: &mut ChartContext<
            '_,
            BitMapBackend<'_>,
            Cartesian2d<RangedCoordf32, RangedCoordf32>,
        >,
        aabb: &Aabb,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let rect = [(aabb.min.x, aabb.min.y), (aabb.max.x, aabb.max.y)];
        chart.draw_series(std::iter::once(Rectangle::new(rect, style)))?;
        // chart.draw(&Rectangle::new(rect, RED.filled()))?;
        Ok(())
    }
}
fn random_color() -> RGBColor {
    let red = u8(..);
    let green = u8(..);
    let blue = u8(..);
    RGBColor(red, green, blue)
}
