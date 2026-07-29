/// Demo contact roster and unassigned handles.

pub const OWNER_PHONE: &str = "+14155559000";

/// How a contact's 1:1 traffic is timed across the 10-year window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    /// Spread across 2016–2026.
    Normal,
    /// Heavy traffic in the past 3 years (2023–2026).
    Frequent,
    /// Mostly older history; little recent traffic.
    Lapsed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageScope {
    /// Individual thread only (no group chat membership).
    OneToOne,
    /// Appears in group chats only (no 1:1 file).
    Group,
    /// Both individual and group threads.
    Both,
}

#[derive(Debug, Clone)]
pub struct Contact {
    pub phones: Vec<String>,
    pub first_name: String,
    pub last_name: String,
    pub exclude: bool,
    pub groups: Vec<String>,
    /// When false, no messages of any kind.
    pub has_messages: bool,
    pub message_scope: MessageScope,
    pub activity: Activity,
    /// Very large 1:1 thread (thousands of messages).
    pub high_volume: bool,
}

#[derive(Debug, Clone)]
pub struct Unassigned {
    pub handle: String,
    pub name_hint: Option<String>,
    pub email_only: bool,
}

#[derive(Debug)]
pub struct Roster {
    pub contacts: Vec<Contact>,
    pub unassigned: Vec<Unassigned>,
    pub exclude_handles: Vec<(String, String)>,
}

/// Extra first/last pairs covering A–Z (display name first letter).
const EXTRA_NAMES: &[(&str, &str)] = &[
    ("Ada", "Vaughn"),
    ("Ben", "Adler"),
    ("Cora", "Bennett"),
    ("Diego", "Cruz"),
    ("Elena", "Diaz"),
    ("Felix", "Edwards"),
    ("Grace", "Finn"),
    ("Hugo", "Garcia"),
    ("Iris", "Holt"),
    ("Jules", "Ibarra"),
    ("Kai", "Jones"),
    ("Lila", "Kwan"),
    ("Nora", "Lane"),
    ("Omar", "Moss"),
    ("Priya", "Nash"),
    ("Ruth", "Ortiz"),
    ("Seth", "Park"),
    ("Uma", "Quincy"),
    ("Vera", "Ross"),
    ("Wade", "Santos"),
    ("Xander", "Torres"),
    ("Yara", "Underwood"),
    ("Zane", "Vargas"),
    ("Amir", "Walsh"),
    ("Beth", "Xu"),
    ("Caleb", "Young"),
    ("Dana", "Zhao"),
    ("Eve", "Abbott"),
    ("Finn", "Brooks"),
    ("Gina", "Carter"),
    ("Hank", "Doyle"),
    ("Ivy", "Ellis"),
    ("Jade", "Frost"),
    ("Kyle", "Grant"),
    ("Leah", "Howard"),
    ("Mia", "Ingram"),
    ("Nate", "Jenkins"),
    ("Olive", "Keller"),
    ("Paul", "Lambert"),
    ("Rosa", "Mills"),
    ("Sean", "Nolan"),
    ("Tess", "Owens"),
    ("Uri", "Perez"),
    ("Vince", "Quinn"),
    ("Wendy", "Ramirez"),
    ("Xiomara", "Steele"),
    ("Yusuf", "Tran"),
    ("Zoe", "Upton"),
    ("Aaron", "Vega"),
    ("Bella", "West"),
];

pub fn build_roster() -> Roster {
    let mut contacts = vec![
        // 1:1 only
        contact(
            &["+14155552301"],
            "Maya",
            "Chen",
            false,
            &["Family"],
            true,
            MessageScope::OneToOne,
        ),
        contact(
            &["+16175553402"],
            "Jordan",
            "Reed",
            false,
            &["Family", "College"],
            true,
            MessageScope::OneToOne,
        ),
        contact(
            &["+19175559008"],
            "Morgan",
            "Patel",
            false,
            &[],
            true,
            MessageScope::OneToOne,
        ),
        contact(
            &["+15035553412"],
            "Avery",
            "Stone",
            false,
            &[],
            true,
            MessageScope::OneToOne,
        ),
        contact(
            &["+12705557816"],
            "Parker",
            "Bell",
            false,
            &[],
            true,
            MessageScope::OneToOne,
        ),
        // Group only
        contact(
            &["+13035555604"],
            "Alex",
            "Martinez",
            false,
            &["Work"],
            true,
            MessageScope::Group,
        ),
        contact(
            &["+12065556705"],
            "Riley",
            "Nguyen",
            false,
            &["College"],
            true,
            MessageScope::Group,
        ),
        contact(
            &["+14805552311"],
            "Quinn",
            "Lopez",
            false,
            &["College"],
            true,
            MessageScope::Group,
        ),
        contact(
            &["+15805554513"],
            "Blake",
            "Turner",
            false,
            &["Family"],
            true,
            MessageScope::Group,
        ),
        contact(
            &["+13125556715"],
            "Skyler",
            "Wright",
            false,
            &["College"],
            true,
            MessageScope::Group,
        ),
        // Both
        contact(
            &["+12125554503"],
            "Sam",
            "Okafor",
            false,
            &["Work"],
            true,
            MessageScope::Both,
        ),
        contact(
            &["+17135557806"],
            "Casey",
            "Brooks",
            false,
            &["Family"],
            true,
            MessageScope::Both,
        ),
        contact(
            &["+18125558907"],
            "Taylor",
            "Singh",
            false,
            &["Work", "College"],
            true,
            MessageScope::Both,
        ),
        contact(
            &["+14045550109"],
            "Jamie",
            "Foster",
            false,
            &["Family"],
            true,
            MessageScope::Both,
        ),
        contact(
            &["+13105551210"],
            "Drew",
            "Kim",
            false,
            &["Work"],
            true,
            MessageScope::Both,
        ),
        contact(
            &["+16025555614"],
            "Cameron",
            "Hayes",
            false,
            &["Work"],
            true,
            MessageScope::Both,
        ),
        contact(
            &["+14245558917"],
            "Reese",
            "Cole",
            false,
            &["Family"],
            true,
            MessageScope::Both,
        ),
        contact(
            &["+15105559018"],
            "Logan",
            "Price",
            false,
            &["Work"],
            true,
            MessageScope::Both,
        ),
        // Excluded with messages (1:1 only)
        contact(
            &["+12145550119"],
            "Old",
            "Roommate",
            true,
            &[],
            true,
            MessageScope::OneToOne,
        ),
        contact(
            &["+18885550120"],
            "Marketing",
            "Blast",
            true,
            &[],
            true,
            MessageScope::OneToOne,
        ),
        contact(
            &["+18005550121"],
            "Telemarketer",
            "X",
            true,
            &[],
            true,
            MessageScope::OneToOne,
        ),
        // Multi-phone
        contact(
            &["+14155552222", "+16505553333"],
            "Pat",
            "MultiPhone",
            false,
            &["Work"],
            true,
            MessageScope::Both,
        ),
        contact(
            &["+13125554444", "+19725556666"],
            "Chris",
            "DualLine",
            false,
            &["Family"],
            true,
            MessageScope::Both,
        ),
        // No messages
        contact(
            &["+17025550126"],
            "Empty",
            "ContactA",
            false,
            &["College"],
            false,
            MessageScope::Both,
        ),
        contact(
            &["+12565550127"],
            "Empty",
            "ContactB",
            false,
            &[],
            false,
            MessageScope::Both,
        ),
        contact(
            &["+19195550128"],
            "Empty",
            "ContactC",
            true,
            &[],
            false,
            MessageScope::Both,
        ),
        contact(
            &["+13175550129"],
            "Empty",
            "ContactD",
            false,
            &["Work"],
            false,
            MessageScope::Both,
        ),
        contact(
            &["+14085550130"],
            "NoChat",
            "Ever",
            false,
            &[],
            false,
            MessageScope::Both,
        ),
    ];

    contacts.push(Contact {
        phones: vec!["+447700900231".into()],
        first_name: "Mononym".into(),
        last_name: String::new(),
        exclude: false,
        groups: vec![],
        has_messages: true,
        message_scope: MessageScope::OneToOne,
        activity: Activity::Normal,
        high_volume: false,
    });
    contacts.push(Contact {
        phones: vec!["+33612345678".into()],
        first_name: String::new(),
        last_name: "SurnameOnly".into(),
        exclude: false,
        groups: vec!["Family".into()],
        has_messages: true,
        message_scope: MessageScope::Group,
        activity: Activity::Normal,
        high_volume: false,
    });

    contacts.extend(extra_contacts());
    assign_activity_patterns(&mut contacts);

    let unassigned = vec![
        Unassigned {
            handle: "+18885559001".into(),
            name_hint: Some("Unknown Caller".into()),
            email_only: false,
        },
        Unassigned {
            handle: "+12125550017".into(),
            name_hint: None,
            email_only: false,
        },
        Unassigned {
            handle: "stranger@demo.example".into(),
            name_hint: Some("Stranger Email".into()),
            email_only: true,
        },
        Unassigned {
            handle: "contractor@demo.example".into(),
            name_hint: Some("Contractor".into()),
            email_only: true,
        },
        Unassigned {
            handle: "+447700900888".into(),
            name_hint: Some("(Unverified)".into()),
            email_only: false,
        },
        Unassigned {
            handle: "+61491570156".into(),
            name_hint: None,
            email_only: false,
        },
    ];

    let exclude_handles = vec![
        ("28747".into(), "BANKALERT".into()),
        ("69243".into(), "PROMO".into()),
        ("737373".into(), "VERIFY".into()),
    ];

    Roster {
        contacts,
        unassigned,
        exclude_handles,
    }
}

/// ~50 generated contacts for alphabet coverage and volume.
fn extra_contacts() -> Vec<Contact> {
    let labels = [&[][..], &["Family"][..], &["Work"][..], &["College"][..]];
    let scopes = [
        MessageScope::OneToOne,
        MessageScope::Both,
        MessageScope::Group,
        MessageScope::Both,
        MessageScope::OneToOne,
    ];

    EXTRA_NAMES
        .iter()
        .enumerate()
        .map(|(i, (first, last))| {
            let phone = format!("+15551{:06}", 100_000 + i);
            let has_messages = i % 17 != 0; // a few empty
            let scope = if !has_messages {
                MessageScope::Both
            } else {
                scopes[i % scopes.len()]
            };
            Contact {
                phones: vec![phone],
                first_name: (*first).into(),
                last_name: (*last).into(),
                exclude: false,
                groups: labels[i % labels.len()]
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect(),
                has_messages,
                message_scope: scope,
                activity: Activity::Normal,
                high_volume: false,
            }
        })
        .collect()
}

/// 15 frequent (incl. 2 high-volume), 10 lapsed — among 1:1-capable contacts.
fn assign_activity_patterns(contacts: &mut [Contact]) {
    let mut one_to_one_idxs: Vec<usize> = contacts
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            c.has_messages
                && !c.exclude
                && matches!(c.message_scope, MessageScope::OneToOne | MessageScope::Both)
        })
        .map(|(i, _)| i)
        .collect();

    // Prefer stable ordering by phone so seed output stays predictable.
    one_to_one_idxs.sort_by_key(|&i| contacts[i].primary_phone().to_string());

    // 2 high-volume whales (also frequent).
    for &i in one_to_one_idxs.iter().take(2) {
        contacts[i].high_volume = true;
        contacts[i].activity = Activity::Frequent;
    }
    // 13 more frequent → 15 total.
    for &i in one_to_one_idxs.iter().skip(2).take(13) {
        contacts[i].activity = Activity::Frequent;
    }
    // 10 lapsed.
    for &i in one_to_one_idxs.iter().skip(15).take(10) {
        contacts[i].activity = Activity::Lapsed;
    }
}

fn contact(
    phones: &[&str],
    first: &str,
    last: &str,
    exclude: bool,
    groups: &[&str],
    has_messages: bool,
    message_scope: MessageScope,
) -> Contact {
    Contact {
        phones: phones.iter().map(|p| (*p).to_string()).collect(),
        first_name: first.into(),
        last_name: last.into(),
        exclude,
        groups: groups.iter().map(|g| (*g).to_string()).collect(),
        has_messages,
        message_scope,
        activity: Activity::Normal,
        high_volume: false,
    }
}

impl Contact {
    pub fn primary_phone(&self) -> &str {
        &self.phones[0]
    }

    pub fn display_hint(&self) -> String {
        let parts = [self.first_name.as_str(), self.last_name.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        parts.join(" ")
    }

    pub fn has_one_to_one(&self) -> bool {
        self.has_messages
            && matches!(
                self.message_scope,
                MessageScope::OneToOne | MessageScope::Both
            )
    }

    pub fn has_group(&self) -> bool {
        self.has_messages
            && !self.exclude
            && matches!(self.message_scope, MessageScope::Group | MessageScope::Both)
    }
}

/// Empty group thread placeholder (header only, no messages).
pub const EMPTY_GROUP_HANDLE: &str = "chat0000000001";

/// Empty 1:1 thread placeholder handle.
pub const EMPTY_THREAD_HANDLE: &str = "+18007438200";

/// Orphaned-message sender stub (not in contacts).
pub const ORPHAN_SENDER: &str = "+447700900999";

/// Synthetic phone-only group participants (not in contacts.csv).
pub fn phone_only_handles(start: usize, count: usize) -> Vec<String> {
    (0..count)
        .map(|i| format!("+15559{:06}", 200_000 + start + i))
        .collect()
}
