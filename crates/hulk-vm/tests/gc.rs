//! Integration tests for the GC: mark-and-sweep reclaims unreachable
//! objects, including ones in reference cycles.

use std::collections::HashMap;

use hulk_ir::{Object, Value};
use hulk_vm::Vm;

fn alloc(vm: &mut Vm, type_name: &str) -> Value {
    let id = vm.heap.alloc(Object {
        type_name: type_name.to_string(),
        fields: HashMap::new(),
    });
    Value::Object(id)
}

#[test]
fn force_gc_with_no_roots_frees_every_object() {
    let mut vm = Vm::new();
    for _ in 0..10 {
        let _ = alloc(&mut vm, "Garbage");
        // not pushed onto the stack or any scope — unreachable.
    }
    assert_eq!(vm.heap.live_count(), 10);
    let freed = vm.force_gc();
    assert_eq!(freed, 10);
    assert_eq!(vm.heap.live_count(), 0);
    // Slots were not deallocated, just marked free for reuse.
    assert_eq!(vm.heap.capacity(), 10);
}

#[test]
fn force_gc_keeps_stack_roots_alive() {
    let mut vm = Vm::new();
    let kept = alloc(&mut vm, "Keep");
    let _ = alloc(&mut vm, "Throw"); // unrooted
    vm.set_stack_for_testing(vec![kept]);
    let freed = vm.force_gc();
    assert_eq!(freed, 1);
    assert_eq!(vm.heap.live_count(), 1);
}

#[test]
fn force_gc_reclaims_self_referential_cycle() {
    // a.next = b; b.next = a; neither rooted → GC must reclaim both.
    let mut vm = Vm::new();
    let a = vm.heap.alloc(Object {
        type_name: "Node".into(),
        fields: HashMap::new(),
    });
    let b = vm.heap.alloc(Object {
        type_name: "Node".into(),
        fields: HashMap::new(),
    });
    vm.heap
        .get_mut(a)
        .fields
        .insert("next".into(), Value::Object(b));
    vm.heap
        .get_mut(b)
        .fields
        .insert("next".into(), Value::Object(a));
    assert_eq!(vm.heap.live_count(), 2);
    let freed = vm.force_gc();
    assert_eq!(freed, 2, "cycle must be reclaimed by mark-and-sweep");
    assert_eq!(vm.heap.live_count(), 0);
}

#[test]
fn force_gc_traces_through_chained_fields() {
    // chain: root -> mid -> leaf. Root only is in scope; both descendants
    // must survive the sweep.
    let mut vm = Vm::new();
    let leaf = vm.heap.alloc(Object {
        type_name: "Leaf".into(),
        fields: HashMap::new(),
    });
    let mut mid_obj = Object {
        type_name: "Mid".into(),
        fields: HashMap::new(),
    };
    mid_obj.fields.insert("next".into(), Value::Object(leaf));
    let mid = vm.heap.alloc(mid_obj);
    let mut root_obj = Object {
        type_name: "Root".into(),
        fields: HashMap::new(),
    };
    root_obj.fields.insert("next".into(), Value::Object(mid));
    let root = vm.heap.alloc(root_obj);
    vm.push_root(Value::Object(root));
    let freed = vm.force_gc();
    assert_eq!(freed, 0);
    assert_eq!(vm.heap.live_count(), 3);
}

#[test]
fn end_to_end_program_with_low_gc_threshold() {
    // Set the GC threshold to 1 so every NewObject triggers a sweep.
    // SAFETY: tests run in a single process; this writes a process-wide var.
    unsafe { std::env::set_var("HULK_GC_THRESHOLD", "1") };
    let source = r#"
        type Box(v: Number) {
            v: Number = v;
            get(): Number => self.v;
        }
        let b = new Box(7) in print(b.get());
    "#;
    let ast = hulk_parser::parse(source).expect("parse");
    hulk_semantic::analyze(&ast).expect("semantic");
    let ir = hulk_ir::lower_program(&ast);
    let result = hulk_vm::Vm::run_program(ir);
    unsafe { std::env::remove_var("HULK_GC_THRESHOLD") };
    assert!(
        result.is_ok(),
        "program failed under aggressive GC: {result:?}"
    );
}
