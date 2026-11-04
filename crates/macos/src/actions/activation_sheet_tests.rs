use super::*;

#[test]
fn a_closed_sheet_is_observed_through_its_retained_owner() {
    let owner = crate::tree::element_for_pid(101);
    let sheet = crate::tree::element_for_pid(102);
    let observation = OwnedSheet {
        owner: owner.clone(),
        sheet: sheet.clone(),
    };
    assert_eq!(
        observation.present_with(|read_owner| {
            assert!(crate::tree::same_element(read_owner, &owner));
            Some(vec![sheet])
        }),
        Some(true)
    );
    assert_eq!(
        observation.present_with(|read_owner| {
            assert!(crate::tree::same_element(read_owner, &owner));
            Some(vec![])
        }),
        Some(false)
    );
    assert!(observation.closed_with(|_| Some(vec![])));
}

#[test]
fn unreadable_owner_is_not_evidence_that_the_sheet_closed() {
    let observation = OwnedSheet {
        owner: crate::tree::element_for_pid(101),
        sheet: crate::tree::element_for_pid(102),
    };
    assert_eq!(observation.present_with(|_| None), None);
    assert!(!observation.closed_with(|_| None));
    assert!(!observation.closed_with(|_| Some(vec![observation.sheet.clone()])));
}

struct FakeTree {
    nodes: Vec<(AXElement, &'static str, Option<AXElement>)>,
}

impl FakeTree {
    fn row_in_sheet(owner_role: &'static str) -> (Self, AXElement, AXElement, AXElement) {
        let row = crate::tree::element_for_pid(201);
        let sheet = crate::tree::element_for_pid(202);
        let owner = crate::tree::element_for_pid(203);
        let nodes = vec![
            (row.clone(), "AXRow", Some(sheet.clone())),
            (sheet.clone(), "AXSheet", Some(owner.clone())),
            (owner.clone(), owner_role, None),
        ];
        (Self { nodes }, row, sheet, owner)
    }

    fn node(&self, element: &AXElement) -> Option<&(AXElement, &'static str, Option<AXElement>)> {
        self.nodes
            .iter()
            .find(|(candidate, _, _)| crate::tree::same_element(candidate, element))
    }

    fn role(&self, element: &AXElement) -> Option<String> {
        self.node(element).map(|(_, role, _)| (*role).to_string())
    }

    fn parent(&self, element: &AXElement) -> Option<AXElement> {
        self.node(element).and_then(|(_, _, parent)| parent.clone())
    }
}

#[test]
fn a_target_inside_a_window_sheet_captures_that_sheet_and_its_owner() {
    let (tree, row, sheet, owner) = FakeTree::row_in_sheet("AXWindow");
    let captured = OwnedSheet::capture_with(
        &row,
        |element| tree.role(element),
        |element| tree.parent(element),
        |_| Some(vec![sheet.clone()]),
    )
    .expect("a sheet listed by its owning window is captured");
    assert!(crate::tree::same_element(&captured.sheet, &sheet));
    assert!(crate::tree::same_element(&captured.owner, &owner));
}

#[test]
fn a_sheet_missing_from_its_owner_at_capture_is_not_tracked() {
    let (tree, row, _, _) = FakeTree::row_in_sheet("AXWindow");
    let without_sheet = OwnedSheet::capture_with(
        &row,
        |element| tree.role(element),
        |element| tree.parent(element),
        |_| Some(vec![]),
    );
    assert!(without_sheet.is_none());
    let unreadable = OwnedSheet::capture_with(
        &row,
        |element| tree.role(element),
        |element| tree.parent(element),
        |_| None,
    );
    assert!(unreadable.is_none());
}

#[test]
fn a_sheet_whose_parent_is_not_a_window_is_not_tracked() {
    let (tree, row, sheet, _) = FakeTree::row_in_sheet("AXGroup");
    let captured = OwnedSheet::capture_with(
        &row,
        |element| tree.role(element),
        |element| tree.parent(element),
        |_| Some(vec![sheet.clone()]),
    );
    assert!(captured.is_none());
}

#[test]
fn a_target_with_no_sheet_within_the_ancestor_walk_is_not_tracked() {
    let looping = crate::tree::element_for_pid(301);
    let reads = std::cell::Cell::new(0);
    let captured = OwnedSheet::capture_with(
        &looping,
        |_| {
            reads.set(reads.get() + 1);
            Some("AXGroup".to_string())
        },
        |element| Some(element.clone()),
        |_| panic!("children are read only for a sheet"),
    );
    assert!(captured.is_none());
    assert_eq!(reads.get(), SHEET_ANCESTOR_WALK);
}

#[test]
fn actions_other_than_axopen_capture_no_sheet() {
    let deadline = agent_desktop_core::Deadline::after(5_000).unwrap();
    let element = crate::tree::element_for_pid(401);
    for action in ["AXPress", "AXConfirm", "AXShowMenu"] {
        assert!(OwnedSheet::capture(&element, action, deadline).is_none());
    }
}
