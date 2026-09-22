//! Accessibility trees for desktop sessions.
//!
//! A Qt application publishes a structured tree of its controls on the
//! accessibility bus: role, name, description, state, and screen bounds. That
//! is the desktop analogue of the browser's DOM, and it is what lets an agent
//! address a control by identity instead of estimating pixel coordinates from a
//! screenshot. Each desktop session gets its own D-Bus connection, so a walk is
//! scoped to that session's application rather than a machine-wide registry.

use anyhow::{Context, Result};
use atspi::connection::AccessibilityConnection;
use atspi::proxy::accessible::AccessibleProxy;
use atspi::proxy::proxy_ext::ProxyExt;
use atspi::zbus;
use atspi::zbus::proxy::CacheProperties;
use atspi::CoordType;
use atspi::{ObjectRefOwned, State};
use serde::Serialize;
use std::time::Duration;

/// The registry's placeholder for a missing object reference.
const NULL_PATH: &str = "/org/a11y/atspi/null";
/// Where the registry daemon exposes the desktop root.
const REGISTRY_ROOT_PATH: &str = "/org/a11y/atspi/accessible/root";
/// The registry daemon's well-known name on the accessibility bus.
const REGISTRY_NAME: &str = "org.a11y.atspi.Registry";
/// Deepest level of the tree to descend.
const MAX_DEPTH: usize = 32;
/// Hard cap on nodes in one walk, so a pathological tree cannot exhaust memory.
const MAX_NODES: usize = 5_000;

/// A node's rectangle in output pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Bounds {
    /// The point at the center of the element, for a pointer action.
    pub fn center(&self) -> (f64, f64) {
        (
            f64::from(self.x) + f64::from(self.width) / 2.0,
            f64::from(self.y) + f64::from(self.height) / 2.0,
        )
    }
}

/// One accessible object, with its children.
#[derive(Debug, Clone, Serialize)]
pub struct Node {
    /// Opaque handle that addresses this object in a later request.
    #[serde(rename = "ref")]
    pub reference: String,
    pub role: String,
    pub name: String,
    pub description: String,
    pub states: Vec<String>,
    /// Absent when the object has no on-screen geometry (for example an
    /// application root).
    pub bounds: Option<Bounds>,
    pub children: Vec<Node>,
}

impl Node {
    /// Find a node by its reference, depth-first.
    pub fn find(&self, reference: &str) -> Option<&Node> {
        if self.reference == reference {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(reference))
    }
}

/// What a walk observed about the tree's content.
///
/// The registry always exposes its own chrome: a desktop root plus one
/// `application` entry per publishing process, and each process entry carries
/// the process name, while its direct children are the application's
/// top-level windows carrying their titles. Counts therefore cover only what
/// the application published inside its windows: `nodes` and `max_depth`
/// describe the application subtrees, and `named` counts named objects
/// strictly below the top-level windows, so neither the process name nor a
/// window title masks a control surface that publishes no names.
/// `named == 0` means nothing the agent could act on carries identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TreeStats {
    /// Applications publishing on the session's bus.
    pub applications: usize,
    /// Objects in the application subtrees, applications included.
    pub nodes: usize,
    /// Named objects strictly below the application's top-level windows.
    pub named: usize,
    /// Deepest application subtree, applications counted as level one.
    pub max_depth: usize,
}

/// A walked tree with its measured content.
pub struct Tree {
    pub root: Node,
    pub stats: TreeStats,
}

impl Tree {
    /// Find a node by its reference, depth-first.
    pub fn find(&self, reference: &str) -> Option<&Node> {
        self.root.find(reference)
    }
}

/// Measure what the application published, given the registry's root node.
fn measure(root: &Node) -> TreeStats {
    fn subtree_size(node: &Node) -> usize {
        1 + node.children.iter().map(subtree_size).sum::<usize>()
    }
    fn subtree_depth(node: &Node) -> usize {
        1 + node.children.iter().map(subtree_depth).max().unwrap_or(0)
    }
    fn named_below(node: &Node) -> usize {
        node.children
            .iter()
            .map(|child| usize::from(!child.name.is_empty()) + named_below(child))
            .sum()
    }
    // Named objects are counted below the top-level windows: the process
    // entry and the window titles are the registry's and the window's own
    // identity, never the identity of a target the agent could act on.
    let named = root
        .children
        .iter()
        .map(|app| app.children.iter().map(named_below).sum::<usize>())
        .sum();
    TreeStats {
        applications: root.children.len(),
        nodes: root.children.iter().map(subtree_size).sum(),
        named,
        max_depth: root.children.iter().map(subtree_depth).max().unwrap_or(0),
    }
}

/// Walk the accessibility tree of the application on `session_address`.
///
/// `session_address` is the session's D-Bus address; the accessibility bus is
/// discovered through it, exactly as a toolkit client would.
pub async fn tree(session_address: &str) -> Result<Tree> {
    let connection = connect(session_address).await?;
    let bus = connection.connection();
    let root = AccessibleProxy::builder(bus)
        .destination(REGISTRY_NAME)
        .context("addressing the accessibility registry")?
        .path(REGISTRY_ROOT_PATH)
        .context("addressing the accessibility registry root")?
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .context("opening the accessibility registry root")?;
    let (root, _) = walk(bus, root, 0, MAX_NODES).await?;
    let stats = measure(&root);
    Ok(Tree { root, stats })
}

/// Connect to a session bus and resolve its accessibility bus.
async fn connect(session_address: &str) -> Result<AccessibilityConnection> {
    let session = zbus::connection::Builder::address(session_address)
        .context("parsing the session bus address")?
        .build()
        .await
        .context("connecting to the session bus")?;
    let reply = session
        .call_method(
            Some("org.a11y.Bus"),
            "/org/a11y/bus",
            Some("org.a11y.Bus"),
            "GetAddress",
            &(),
        )
        .await
        .context("asking the session bus for the accessibility bus")?;
    let address: String = reply
        .body()
        .deserialize()
        .context("decoding the accessibility bus address")?;
    let address =
        zbus::Address::try_from(address.as_str()).context("parsing the accessibility address")?;
    AccessibilityConnection::from_address(address)
        .await
        .context("connecting to the accessibility bus")
}

/// A boxed recursive walk future: the tree is walked depth-first, so the
/// function returns a pinned future rather than recursing through the type.
type WalkFuture<'c> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<(Node, usize)>> + Send + 'c>>;

/// How long to wait for a session's registry daemon to answer.
const REGISTRY_READY_TIMEOUT: Duration = Duration::from_secs(10);

/// Wait until the session's accessibility registry answers.
///
/// A desktop session starts its registry eagerly, because the registry cannot
/// be relied on to start lazily: the bus launcher hands activation to systemd
/// whenever systemd is booted, and systemd can only activate services on the
/// real session bus, never on a private one. Poll a trivial property until the
/// registry name has an owner.
pub async fn await_registry(session_address: &str) -> Result<()> {
    let connection = connect(session_address).await?;
    let root = connection
        .root_accessible_on_registry()
        .await
        .context("opening the accessibility registry root")?;
    let deadline = tokio::time::Instant::now() + REGISTRY_READY_TIMEOUT;
    loop {
        match root.child_count().await {
            Ok(_) => return Ok(()),
            Err(err) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(err).context("waiting for the accessibility registry");
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

/// Read one node and recurse into its children, spending at most `budget`
/// nodes including this one. Returns the node and the budget that remains.
fn walk<'c>(
    bus: &'c zbus::Connection,
    node: AccessibleProxy<'c>,
    depth: usize,
    budget: usize,
) -> WalkFuture<'c> {
    Box::pin(async move {
        let reference = reference_of(&node);
        // Every field is best-effort: one unresponsive child must not fail the
        // whole walk. The proxy disables property caching, without which zbus
        // returns empty names from the registry's stale cache.
        let role = node.get_role_name().await.unwrap_or_default();
        let name = node.name().await.unwrap_or_default();
        let description = node.description().await.unwrap_or_default();
        let states = state_names(&node).await;
        let bounds = node_bounds(&node).await;

        let mut remaining = budget;
        remaining = remaining.saturating_sub(1);
        let mut children = Vec::new();
        if depth < MAX_DEPTH && remaining > 0 {
            for child in node.get_children().await.unwrap_or_default() {
                if remaining == 0 {
                    break;
                }
                let Some(proxy) = child_proxy(bus, &child).await else {
                    continue;
                };
                let (child_node, left) = walk(bus, proxy, depth + 1, remaining).await?;
                remaining = left;
                children.push(child_node);
            }
        }

        Ok((
            Node {
                reference,
                role,
                name,
                description,
                states,
                bounds,
                children,
            },
            remaining,
        ))
    })
}

/// Address a child object from its reference on the same connection.
async fn child_proxy<'c>(
    bus: &'c zbus::Connection,
    child: &ObjectRefOwned,
) -> Option<AccessibleProxy<'c>> {
    let name = child.name()?.clone();
    let path = child.path().clone();
    if path.as_str() == NULL_PATH {
        return None;
    }
    AccessibleProxy::builder(bus)
        .destination(name)
        .ok()?
        .path(path)
        .ok()?
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .ok()
}

/// The opaque handle for one accessible object: its bus name and object path.
fn reference_of(node: &AccessibleProxy<'_>) -> String {
    format!("{}|{}", node.inner().destination(), node.inner().path())
}

/// The human-readable state names an object currently holds.
async fn state_names(node: &AccessibleProxy<'_>) -> Vec<String> {
    node.get_state()
        .await
        .map(|set| {
            set.iter()
                .map(|state: State| state.to_static_str().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// The object's rectangle in output pixels, if it has a component interface.
async fn node_bounds(node: &AccessibleProxy<'_>) -> Option<Bounds> {
    let proxies = node.proxies().await.ok()?;
    let component = proxies.component().await.ok()?;
    let (x, y, width, height) = component.get_extents(CoordType::Screen).await.ok()?;
    Some(Bounds {
        x,
        y,
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(reference: &str, role: &str) -> Node {
        Node {
            reference: reference.into(),
            role: role.into(),
            name: String::new(),
            description: String::new(),
            states: Vec::new(),
            bounds: None,
            children: Vec::new(),
        }
    }

    fn named(reference: &str, role: &str, name: &str) -> Node {
        let mut node = leaf(reference, role);
        node.name = name.into();
        node
    }

    #[test]
    fn measure_counts_what_the_application_publishes() {
        let mut frame = named("frame", "frame", "Lumen Qt Fixture");
        let mut filler = leaf("filler", "filler");
        let mut text = named("l2", "text", "Name field");
        text.children.push(named("l4", "label", "Name"));
        filler.children = vec![
            named("l1", "label", "0"),
            text,
            named("l3", "push button", "Increment"),
        ];
        frame.children.push(filler);
        let mut app = named("app", "application", "qt-fixture");
        app.children.push(frame);
        let mut root = named("root", "desktop frame", "main");
        root.children.push(app);

        let stats = measure(&root);
        assert_eq!(stats.applications, 1);
        // application, frame, filler, label, text, text's label, button.
        assert_eq!(stats.nodes, 7);
        // The window title does not count: only controls inside the window.
        assert_eq!(stats.named, 4);
        // application > frame > filler > text > text's label.
        assert_eq!(stats.max_depth, 5);
    }

    #[test]
    fn measure_reports_zero_named_for_a_titled_window_over_unnamed_controls() {
        // A window title names the window, not a target: even with a title,
        // an unnamed control surface must measure zero named objects.
        let mut filler = leaf("filler", "filler");
        filler.children.push(leaf("button", "push button"));
        let mut frame = named("frame", "frame", "Painted Canvas");
        frame.children.push(filler);
        let mut app = named("app", "application", "qt-loader");
        app.children.push(frame);
        let mut root = named("root", "desktop frame", "main");
        root.children.push(app);

        let stats = measure(&root);
        assert_eq!(stats.applications, 1);
        assert_eq!(stats.nodes, 4);
        assert_eq!(stats.named, 0);
        assert_eq!(stats.max_depth, 4);
    }

    #[test]
    fn measure_reports_zero_named_for_a_chrome_only_application() {
        // A bare rectangle publishes no accessible object at all, so only the
        // window chrome remains, unnamed below the application entry.
        let mut frame = leaf("frame", "frame");
        frame.children.push(leaf("filler", "filler"));
        let mut app = named("app", "application", "qt-loader");
        app.children.push(frame);
        let mut root = named("root", "desktop frame", "main");
        root.children.push(app);

        let stats = measure(&root);
        assert_eq!(stats.applications, 1);
        assert_eq!(stats.nodes, 3);
        assert_eq!(stats.named, 0);
        assert_eq!(stats.max_depth, 3);
    }

    #[test]
    fn measure_reports_no_application_after_the_app_exits() {
        // The registry drops a departed application and keeps only its root.
        let root = named("root", "desktop frame", "main");
        let stats = measure(&root);
        assert_eq!(stats.applications, 0);
        assert_eq!(stats.nodes, 0);
        assert_eq!(stats.named, 0);
        assert_eq!(stats.max_depth, 0);
    }

    #[test]
    fn measure_ignores_the_registry_process_names() {
        // The desktop root and the application entry always carry names; they
        // are registry chrome, not published content.
        let mut app = named("app", "application", "qt-loader");
        app.children.push(leaf("frame", "frame"));
        let mut root = named("root", "desktop frame", "main");
        root.children.push(app);
        let stats = measure(&root);
        assert_eq!(stats.named, 0);
        assert_eq!(stats.max_depth, 2);
    }

    #[test]
    fn center_is_the_middle_of_the_rectangle() {
        let bounds = Bounds {
            x: 100,
            y: 50,
            width: 80,
            height: 24,
        };
        assert_eq!(bounds.center(), (140.0, 62.0));
    }

    #[test]
    fn find_searches_children_depth_first() {
        let mut root = leaf("root", "frame");
        root.children.push(leaf("a", "label"));
        let mut middle = leaf("b", "panel");
        middle.children.push(leaf("c", "button"));
        root.children.push(middle);

        assert_eq!(
            root.find("c").map(|node| node.role.as_str()),
            Some("button")
        );
        assert_eq!(root.find("a").map(|node| node.role.as_str()), Some("label"));
        assert!(root.find("missing").is_none());
    }
}
