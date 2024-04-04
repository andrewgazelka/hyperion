use std::collections::HashSet;

use itertools::Itertools;

use super::*;
use crate::aabb::CheckableAabb;

// todo: consider duplicate
fn collisions_naive(
    elements: &[Aabb],
    target: Aabb,
) -> Result<HashSet<CheckableAabb>, ordered_float::FloatIsNan> {
    elements
        .iter()
        .filter(move |elem| elem.collides(&target))
        .copied()
        .map(CheckableAabb::try_from)
        .try_collect()
}

#[test]
fn test_build_all_sizes() {
    let counts = &[10, 100];

    for count in counts {
        let elements = create_random_elements_1(*count, 100.0);
        Bvh::build::<TrivialHeuristic>(elements);
    }
}

#[test]
fn test_query_one() {
    let mut elements = create_random_elements_1(10_000, 100.0);

    // for elem in &mut elements {
    //     elem.min.z = -0.0001;
    //     elem.max.z = 0.0001;
    // }

    let mut elems = elements.clone();
    let bvh = Bvh::build::<TrivialHeuristic>(elems);

    let element = random_aabb(30.0);

    println!("element: {}", element);

    let naive_collisions = collisions_naive(&elements, element).unwrap();

    let mut num_collisions = 0;

    let mut bvh_collisions = Vec::new();
    // 1000 x 1000 x 1000 = 1B ... 1B / 1M = 1000 blocks on average...
    // on average num_collisions should be super low
    bvh.get_collisions(element, |elem| {
        num_collisions += 1;
        assert!(elem.collides(&element));
        bvh_collisions.push(CheckableAabb::try_from(*elem).unwrap());
    });

    for elem in &naive_collisions {
        assert!(bvh_collisions.contains(elem));
    }

    assert_eq!(num_collisions, naive_collisions.len());

    // bvh.plot("test.png").unwrap()
}

#[test]
fn test_query_all() {
    let mut elements = create_random_elements_1(10_000, 100.0);
    let bvh = Bvh::build::<TrivialHeuristic>(elements);

    let node_count = bvh.nodes.len();
    println!("node count: {}", node_count);

    let mut num_collisions = 0;

    bvh.get_collisions(Aabb::EVERYTHING, |_| {
        num_collisions += 1;
    });

    assert_eq!(num_collisions, 10_000);
}

#[test]
fn children_returns_none_when_no_children() {
    let node = BvhNode {
        aabb: Aabb::NULL,
        left: None,
        right: None,
    };
    let bvh: Bvh<i32> = Bvh {
        nodes: Vec::new(),
        elements: Vec::new(),
        root: None,
    };
    assert!(node.children(&bvh).next().is_none());
}

#[test]
fn children_returns_internal_nodes() {
    let aabb = random_aabb(100.0);

    let child_node = BvhNode {
        aabb,
        left: None,
        right: None,
    };

    let node = BvhNode {
        aabb: aabb.expand(10.0),
        left: Some(NonZeroI32::new(1).unwrap()),
        right: Some(NonZeroI32::new(2).unwrap()),
    };

    let bvh: Bvh<i32> = Bvh {
        nodes: vec![BvhNode::DUMMY, child_node, child_node],
        elements: Vec::new(),
        root: None,
    };
    let mut children = node.children(&bvh);
    assert_eq!(children.next(), Some(Node::Internal(&child_node)));
    assert_eq!(children.next(), Some(Node::Internal(&child_node)));
    assert!(children.next().is_none());
}

#[test]
fn get_closest_returns_closest_element() {
    let mut elements = vec![
        Aabb::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)),
        Aabb::new(Vec3::new(2.0, 2.0, 2.0), Vec3::new(3.0, 3.0, 3.0)),
        Aabb::new(Vec3::new(4.0, 4.0, 4.0), Vec3::new(5.0, 5.0, 5.0)),
    ];
    let bvh = Bvh::build::<TrivialHeuristic>(elements);

    let target = Vec3::new(2.5, 2.5, 2.5);
    let closest = bvh.get_closest(target);

    assert!(closest.is_some());
    let (closest_element, _) = closest.unwrap();
    assert_eq!(
        closest_element.aabb(),
        Aabb::new(Vec3::new(2.0, 2.0, 2.0), Vec3::new(3.0, 3.0, 3.0))
    );
}

#[test]
fn get_closest_returns_closest_element_with_random_data() {
    let mut elements: Vec<Aabb> = (0..1000)
        .map(|_| {
            let min = Vec3::new(
                fastrand::f32().mul_add(200.0, -100.0),
                fastrand::f32().mul_add(200.0, -100.0),
                fastrand::f32().mul_add(200.0, -100.0),
            );
            let max = min + Vec3::new(1.0, 1.0, 1.0);
            Aabb::new(min, max)
        })
        .collect();

    let bvh = Bvh::build::<TrivialHeuristic>(elements.clone());

    let target = Vec3::new(
        fastrand::f32().mul_add(200.0, -100.0),
        fastrand::f32().mul_add(200.0, -100.0),
        fastrand::f32().mul_add(200.0, -100.0),
    );
    let closest = bvh.get_closest(target);

    assert!(closest.is_some());
    let (closest_element, _) = closest.unwrap();

    // Check that the closest element is indeed the closest by comparing with all elements
    for element in &elements {
        assert!(element.aabb().dist2(target) >= closest_element.aabb().dist2(target));
    }
}

#[test]
fn get_closest_returns_none_when_no_elements() {
    let mut elements: Vec<Aabb> = vec![];
    let bvh = Bvh::build::<TrivialHeuristic>(elements.clone());

    let target = Vec3::new(2.5, 2.5, 2.5);
    let closest = bvh.get_closest(target);

    assert!(closest.is_none());
}

// #[test]
// fn children_returns_leaf_nodes() {
//     let node = BvhNode {
//         aabb: Aabb::NULL,
//         left: Some(NonZeroI32::new(-1).unwrap()),
//         right: Some(NonZeroI32::new(-2).unwrap()),
//     };
//     let bvh: Bvh<i32> = Bvh {
//         nodes: vec![BvhNode::DUMMY],
//         elems: Cow::Borrowed(&[
//             child_elems.clone(),
//             child_elems.clone(),
//         ]),
//         root: None,
//     };
//     let mut children = node.children(&bvh);
//     assert_eq!(children.next(), Some(Node::Leaf(&child_elems)));
//     assert_eq!(children.next(), Some(Node::Leaf(&child_elems)));
// }
