// SPDX-License-Identifier: GPL-3.0-or-later
//! "Encontrar su ecualizador": a blind listening test in five steps. Each step offers five versions
//! of one frequency region (much less to much more) in random order; the picks add up to a custom curve
//! that is written to one of the user presets while the music plays.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;
use serde_json::json;

use super::Ui;

/// (title, what changes, weight of each band in protocol order: Clear Bass, 400, 1k, 2.5k, 6.3k, 16k)
const STEPS: [(&str, &str, [f64; 6]); 5] = [
    ("Graves", "El cuerpo y la pegada de los graves", [1.0, 0.6, 0.0, 0.0, 0.0, 0.0]),
    ("Medios graves", "La calidez de los instrumentos y las voces graves", [0.3, 1.0, 0.5, 0.0, 0.0, 0.0]),
    ("Medios", "La claridad de las voces", [0.0, 0.3, 1.0, 0.6, 0.0, 0.0]),
    ("Presencia", "El detalle y la cercanía", [0.0, 0.0, 0.2, 1.0, 0.6, 0.2]),
    ("Agudos", "El brillo y el aire", [0.0, 0.0, 0.0, 0.3, 0.8, 1.0]),
];
const AMOUNTS: [f64; 5] = [-6.0, -3.0, 0.0, 3.0, 6.0];
const TARGETS: [(u8, &str); 2] = [(0xA1, "Personalizado 1"), (0xA2, "Personalizado 2")];
const LABELS: [&str; 5] = ["A", "B", "C", "D", "E"];

/// A shuffled order of the five amounts: stride 1..4 is coprime with 5, so every amount appears once.
fn order(seed: usize, step: usize) -> [usize; 5] {
    let (stride, offset) = (1 + (seed + step) % 4, (seed / 4 + step * 3) % 5);
    std::array::from_fn(|i| (offset + i * stride) % 5)
}

/// `base` with one step's region moved by `amount`, clamped to the headset's −10..10.
fn candidate(base: &[i64; 6], step: usize, amount: f64) -> [i64; 6] {
    let mut out = *base;
    for (v, w) in out.iter_mut().zip(STEPS[step].2) {
        *v = (*v as f64 + w * amount).round().clamp(-10.0, 10.0) as i64;
    }
    out
}

struct Run {
    ui: Rc<Ui>,
    dialog: adw::Dialog,
    stack: gtk::Stack,
    target: Cell<u8>,
    original: RefCell<Option<(u8, Vec<i8>)>>, // preset that was active, and the target's own bands
    bases: RefCell<Vec<[i64; 6]>>,            // curve each step starts from; bases[n + 1] is the pick of step n
    seed: usize,
    finished: Cell<bool>,
}

pub fn run(ui: &Rc<Ui>) {
    let dialog = adw::Dialog::builder().title("Encontrar su ecualizador").content_width(440).build();
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    let stack = gtk::Stack::builder().transition_type(gtk::StackTransitionType::SlideLeft).build();
    view.set_content(Some(&stack));
    dialog.set_child(Some(&view));
    let seed = glib::real_time() as usize / 1000;
    let run = Rc::new(Run {
        ui: ui.clone(),
        dialog: dialog.clone(),
        stack,
        target: Cell::new(TARGETS[0].0),
        original: RefCell::default(),
        bases: RefCell::new(vec![[0; 6]]),
        seed,
        finished: Cell::new(false),
    });
    run.intro();
    let r = run.clone();
    dialog.connect_closed(move |_| r.restore_unless_finished());
    dialog.present(Some(&ui.window));
}

impl Run {
    fn page(&self, child: &impl IsA<gtk::Widget>) {
        let name = format!("p{}", glib::real_time());
        self.stack.add_named(child, Some(&name));
        self.stack.set_visible_child_name(&name);
    }

    fn intro(self: &Rc<Self>) {
        let page = adw::StatusPage::builder()
            .icon_name("audio-speakers-symbolic")
            .title("Encuentra tu sonido")
            .description(
                "Pon música que te guste y escúchala con los auriculares puestos. En cinco pasos \
                 elige la versión que mejor te suene; el resultado se guarda como ajuste personalizado.",
            )
            .build();
        let body = gtk::Box::new(gtk::Orientation::Vertical, 18);
        let targets = adw::ToggleGroup::builder().halign(gtk::Align::Center).build();
        for (id, label) in TARGETS {
            targets.add(adw::Toggle::builder().name(id.to_string()).label(label).build());
        }
        targets.set_active_name(Some(&TARGETS[0].0.to_string()));
        let start = gtk::Button::builder().label("Empezar").halign(gtk::Align::Center).build();
        start.add_css_class("pill");
        start.add_css_class("suggested-action");
        body.append(&gtk::Label::new(Some("Guardar el resultado en")));
        body.append(&targets);
        body.append(&start);
        page.set_child(Some(&body));
        self.page(&page);

        let r = self.clone();
        start.connect_clicked(move |b| {
            let target = targets.active_name().and_then(|n| n.parse().ok()).unwrap_or(TARGETS[0].0);
            b.set_sensitive(false);
            r.start(target);
        });
    }

    /// Switches to the target preset and waits until the headset reports it, so band writes land there.
    fn start(self: &Rc<Self>, target: u8) {
        self.target.set(target);
        let previous = self.ui.last.borrow().eq_preset.unwrap_or(0);
        self.ui.set("eq_preset", json!(target));
        let (r, tries) = (self.clone(), Cell::new(0));
        glib::timeout_add_local(Duration::from_millis(150), move || {
            let last = r.ui.last.borrow().clone();
            if last.eq_preset == Some(target) && last.eq.is_some() {
                *r.original.borrow_mut() = Some((previous, last.eq.unwrap_or_default()));
                r.step(0);
                return glib::ControlFlow::Break;
            }
            tries.set(tries.get() + 1);
            if tries.get() > 30 {
                r.ui.toast.add_toast(adw::Toast::new("Los auriculares no cambiaron de ecualizador"));
                r.dialog.close();
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    }

    fn step(self: &Rc<Self>, n: usize) {
        // entering a step (forward or back) starts again from the curve before it
        let back = self.bases.borrow().len() > n + 1;
        self.bases.borrow_mut().truncate(n + 1);
        let base = self.bases.borrow()[n];
        self.ui.set("eq", json!(base));
        self.stack.set_transition_type(if back {
            gtk::StackTransitionType::SlideRight
        } else {
            gtk::StackTransitionType::SlideLeft
        });
        let (title, description, _) = STEPS[n];
        let body = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .build();
        let dots = gtk::Box::builder().spacing(8).halign(gtk::Align::Center).build();
        for i in 0..STEPS.len() {
            let dot = gtk::Label::new(Some("●"));
            dot.add_css_class(if i <= n { "accent" } else { "dim-label" });
            dots.append(&dot);
        }
        body.append(&dots);
        let heading = gtk::Label::new(Some(&format!("Paso {} de {}: {title}", n + 1, STEPS.len())));
        heading.add_css_class("title-2");
        body.append(&heading);
        let hint = gtk::Label::builder()
            .label(format!("{description}. Escucha las cinco versiones (A a E) y elige la que más te guste."))
            .wrap(true)
            .justify(gtk::Justification::Center)
            .build();
        hint.add_css_class("dim-label");
        body.append(&hint);

        let options = adw::ToggleGroup::builder().halign(gtk::Align::Center).build();
        options.add_css_class("round");
        for label in LABELS {
            options.add(adw::Toggle::builder().name(label).label(label).build());
        }
        options.set_active(gtk::INVALID_LIST_POSITION);
        body.append(&options);
        let next = gtk::Button::builder()
            .label(if n + 1 == STEPS.len() { "Terminar" } else { "Siguiente" })
            .halign(gtk::Align::Center)
            .sensitive(false)
            .build();
        next.add_css_class("pill");
        next.add_css_class("suggested-action");
        let buttons = gtk::Box::builder().spacing(12).halign(gtk::Align::Center).build();
        if n > 0 {
            let previous = gtk::Button::builder().label("Atrás").build();
            previous.add_css_class("pill");
            let r = self.clone();
            previous.connect_clicked(move |_| r.step(n - 1));
            buttons.append(&previous);
        }
        buttons.append(&next);
        body.append(&buttons);
        self.page(&body);

        // the same five amounts every step, in an order the listener cannot predict
        let order = order(self.seed, n);
        let chosen = Rc::new(Cell::new(base));
        let (r, c, next_button) = (self.clone(), chosen.clone(), next.clone());
        options.connect_active_notify(move |g| {
            let i = g.active() as usize; // INVALID_LIST_POSITION when nothing is picked
            if i >= order.len() {
                return;
            }
            let curve = candidate(&base, n, AMOUNTS[order[i]]);
            c.set(curve);
            r.ui.set("eq", json!(curve));
            next_button.set_sensitive(true);
        });
        let r = self.clone();
        next.connect_clicked(move |_| {
            r.bases.borrow_mut().push(chosen.get());
            if n + 1 < STEPS.len() {
                r.step(n + 1);
            } else {
                r.done();
            }
        });
    }

    fn done(self: &Rc<Self>) {
        self.finished.set(true);
        let name = TARGETS.iter().find(|(id, _)| *id == self.target.get()).map_or("", |(_, l)| *l);
        let page = adw::StatusPage::builder()
            .icon_name("object-select-symbolic")
            .title("¡Listo!")
            .description(format!("Tu ecualizador quedó guardado en {name} y ya está activo."))
            .build();
        let close = gtk::Button::builder().label("Cerrar").halign(gtk::Align::Center).build();
        close.add_css_class("pill");
        page.set_child(Some(&close));
        self.page(&page);
        let d = self.dialog.clone();
        close.connect_clicked(move |_| {
            d.close();
        });
    }

    /// Leaving early puts back the target's bands and the preset that was active before.
    fn restore_unless_finished(&self) {
        if self.finished.get() {
            return;
        }
        if let Some((preset, bands)) = self.original.borrow_mut().take() {
            self.ui.set("eq", json!(bands));
            self.ui.set("eq_preset", json!(preset));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_move_one_region_and_stay_in_range() {
        assert_eq!(candidate(&[0; 6], 0, 6.0), [6, 4, 0, 0, 0, 0]);
        assert_eq!(candidate(&[0; 6], 4, -4.0), [0, 0, 0, -1, -3, -4]);
        assert_eq!(candidate(&[9; 6], 2, 4.0), [9, 10, 10, 10, 9, 9]);
        assert_eq!(candidate(&[3; 6], 1, 0.0), [3; 6]);
    }

    #[test]
    fn every_step_offers_all_five_amounts() {
        for seed in 0..40 {
            for n in 0..STEPS.len() {
                let mut seen = order(seed, n);
                seen.sort();
                assert_eq!(seen, [0, 1, 2, 3, 4]);
            }
        }
    }
}
