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

/// Walk the accessibility tree of the application on `session_address`.
///
/// `session_address` is the session's D-Bus address; the accessibility bus is
/// discovered through it, exactly as a toolkit client would.
pub async fn tree(session_address: &str) -> Result<Node> {
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
    let (node, _) = walk(bus, root, 0, MAX_NODES).await?;
    Ok(node)
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
