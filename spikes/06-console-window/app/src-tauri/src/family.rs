//! Canned family data, held in memory. It stands in for the real core and the
//! hub store, which this proof must not use. The rules are deliberately the
//! same as the mock in `src/host.ts`.

use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub locked: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChildToday {
    pub id: String,
    pub name: String,
    pub used_minutes: u32,
    pub left_minutes: u32,
    pub locked: bool,
    pub devices: Vec<Device>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FamilyView {
    pub children: Vec<ChildToday>,
    pub answered_by: String,
    pub revision: u64,
}

struct Child {
    id: &'static str,
    name: &'static str,
    used_minutes: u32,
    budget_minutes: u32,
    extra_minutes: u32,
    locked: bool,
    devices: Vec<Device>,
}

pub struct Family {
    children: Vec<Child>,
    revision: u64,
}

fn device(id: &str, name: &str, kind: &str) -> Device {
    Device { id: id.into(), name: name.into(), kind: kind.into(), locked: false }
}

impl Family {
    pub fn canned() -> Self {
        Family {
            revision: 1,
            children: vec![
                Child {
                    id: "c1",
                    name: "Asha",
                    used_minutes: 45,
                    budget_minutes: 120,
                    extra_minutes: 0,
                    locked: false,
                    devices: vec![
                        device("d1", "Tablet", "Android"),
                        device("d2", "Family PC, Asha", "Windows account"),
                    ],
                },
                Child {
                    id: "c2",
                    name: "Kabir",
                    used_minutes: 80,
                    budget_minutes: 90,
                    extra_minutes: 0,
                    locked: false,
                    devices: vec![
                        device("d3", "Phone", "Android"),
                        device("d4", "Family PC, Kabir", "Windows account"),
                    ],
                },
            ],
        }
    }

    pub fn view(&self) -> FamilyView {
        FamilyView {
            answered_by: "rust".into(),
            revision: self.revision,
            children: self
                .children
                .iter()
                .map(|c| ChildToday {
                    id: c.id.into(),
                    name: c.name.into(),
                    used_minutes: c.used_minutes,
                    left_minutes: (c.budget_minutes + c.extra_minutes).saturating_sub(c.used_minutes),
                    locked: c.locked,
                    devices: c.devices.clone(),
                })
                .collect(),
        }
    }

    fn child(&mut self, child_id: &str) -> Result<&mut Child, String> {
        self.children
            .iter_mut()
            .find(|c| c.id == child_id)
            .ok_or_else(|| format!("no child with id {child_id}"))
    }

    pub fn give_minutes(&mut self, child_id: &str, minutes: u32) -> Result<FamilyView, String> {
        if !(1..=240).contains(&minutes) {
            return Err("minutes must be a whole number from 1 to 240".into());
        }
        let child = self.child(child_id)?;
        child.extra_minutes += minutes;
        child.locked = false;
        child.devices.iter_mut().for_each(|d| d.locked = false);
        self.revision += 1;
        Ok(self.view())
    }

    pub fn lock_child(&mut self, child_id: &str) -> Result<FamilyView, String> {
        let child = self.child(child_id)?;
        child.locked = true;
        child.devices.iter_mut().for_each(|d| d.locked = true);
        self.revision += 1;
        Ok(self.view())
    }
}
