#include "accessibility.h"

#include <QDBusArgument>
#include <QDBusConnection>
#include <QDBusInterface>
#include <QDBusMessage>
#include <QDBusVariant>
#include <QElapsedTimer>
#include <QJsonArray>
#include <QThread>
#include <QVariant>

namespace {

constexpr int kMaxDepth = 32;
constexpr int kMaxNodes = 5000;
constexpr char kRegistryName[] = "org.a11y.atspi.Registry";
constexpr char kRegistryRootPath[] = "/org/a11y/atspi/accessible/root";
constexpr char kNullPath[] = "/org/a11y/atspi/null";

struct Bounds {
    int x = 0;
    int y = 0;
    int width = 0;
    int height = 0;
    bool valid = false;
};

/// One accessible object as read from the bus.
struct Node {
    QString reference;
    QString role;
    QString name;
    QString description;
    QJsonArray children;
    Bounds bounds;
};

/// Read the accessibility bus address, discovered through the session bus.
QString accessibilityBus(const QString& sessionAddress, QString* error) {
    const QString connectionName =
        QStringLiteral("lumen-a11y-lookup-%1").arg(qHash(sessionAddress));
    QDBusConnection connection = QDBusConnection::connectToBus(sessionAddress, connectionName);
    if (!connection.isConnected()) {
        if (error) {
            *error = QStringLiteral("could not connect to the session bus");
        }
        return QString();
    }
    QDBusInterface bus(QStringLiteral("org.a11y.Bus"), QStringLiteral("/org/a11y/bus"),
                       QStringLiteral("org.a11y.Bus"), connection);
    const QDBusMessage reply = bus.call(QStringLiteral("GetAddress"));
    if (reply.type() != QDBusMessage::ReplyMessage || reply.arguments().isEmpty()) {
        if (error) {
            *error = QStringLiteral("the session bus did not provide an accessibility bus");
        }
        return QString();
    }
    return reply.arguments().first().toString();
}

QString busNameFor(const QString& address) {
    return QStringLiteral("lumen-a11y-%1").arg(qHash(address));
}

/// One child reference on the accessibility bus: a (bus name, object path) pair.
///
/// AT-SPI returns these as the D-Bus signature `(so)`, which arrives as a
/// QDBusArgument rather than a QVariantList; reading it as the latter silently
/// yields no children at all, which is how a populated tree looks empty.
struct ObjectRef {
    QString name;
    QString path;
};

QDBusArgument& operator>>(const QDBusArgument& argument, ObjectRef& ref) {
    argument.beginStructure();
    argument >> ref.name >> ref.path;
    argument.endStructure();
    return const_cast<QDBusArgument&>(argument);
}

QDBusArgument& operator<<(QDBusArgument& argument, const ObjectRef& ref) {
    argument.beginStructure();
    argument << ref.name << ref.path;
    argument.endStructure();
    return argument;
}

/// Read a child list from a reply argument of signature `a(so)`.
QList<ObjectRef> readChildren(const QVariant& argument) {
    QList<ObjectRef> children;
    const QDBusArgument dbusArgument = argument.value<QDBusArgument>();
    dbusArgument.beginArray();
    while (!dbusArgument.atEnd()) {
        ObjectRef ref;
        dbusArgument >> ref;
        children.append(ref);
    }
    dbusArgument.endArray();
    return children;
}

/// The rectangle of an object, when it has a component interface.
Bounds readBounds(QDBusConnection& bus, const QString& destination, const QString& path) {
    Bounds bounds;
    QDBusInterface component(destination, path, QStringLiteral("org.a11y.atspi.Component"), bus);
    const QDBusMessage reply = component.call(QStringLiteral("GetExtents"), uint(0));
    if (reply.type() != QDBusMessage::ReplyMessage || reply.arguments().isEmpty()) {
        return bounds;
    }
    // (x, y, width, height) as four ints.
    const QDBusArgument argument = reply.arguments().first().value<QDBusArgument>();
    int x = 0;
    int y = 0;
    int width = 0;
    int height = 0;
    argument.beginStructure();
    argument >> x >> y >> width >> height;
    argument.endStructure();
    bounds = {x, y, width, height, true};
    return bounds;
}

/// Read a string that may arrive either bare or wrapped in a D-Bus variant.
///
/// AT-SPI's `GetName` and `GetDescription` are declared as returning `v`, so the
/// value arrives as a QDBusVariant. Qt registers QDBusVariant as its own
/// metatype (not QMetaType::QVariant), and `toString()` on it yields an empty
/// string — which makes every object in a populated tree look nameless.
QString stringValue(const QVariant& argument) {
    if (argument.metaType().id() == qMetaTypeId<QDBusVariant>()) {
        return argument.value<QDBusVariant>().variant().toString();
    }
    return argument.toString();
}

QJsonObject walk(QDBusConnection& bus, const QString& destination, const QString& path, int depth,
                 int* budget) {
    QJsonObject node;
    node[QStringLiteral("ref")] = destination + QLatin1Char('|') + path;
    node[QStringLiteral("role")] = QString();
    node[QStringLiteral("name")] = QString();
    node[QStringLiteral("description")] = QString();

    QDBusInterface accessible(destination, path, QStringLiteral("org.a11y.atspi.Accessible"), bus);
    const QDBusMessage roleReply = accessible.call(QStringLiteral("GetRoleName"));
    if (roleReply.type() == QDBusMessage::ReplyMessage && !roleReply.arguments().isEmpty()) {
        node[QStringLiteral("role")] = stringValue(roleReply.arguments().first());
    }
    const QDBusMessage nameReply = accessible.call(QStringLiteral("GetName"));
    if (nameReply.type() == QDBusMessage::ReplyMessage && !nameReply.arguments().isEmpty()) {
        node[QStringLiteral("name")] = stringValue(nameReply.arguments().first());
    }
    const QDBusMessage descReply = accessible.call(QStringLiteral("GetDescription"));
    if (descReply.type() == QDBusMessage::ReplyMessage && !descReply.arguments().isEmpty()) {
        node[QStringLiteral("description")] = stringValue(descReply.arguments().first());
    }

    const Bounds bounds = readBounds(bus, destination, path);
    if (bounds.valid) {
        QJsonObject rect;
        rect[QStringLiteral("x")] = bounds.x;
        rect[QStringLiteral("y")] = bounds.y;
        rect[QStringLiteral("width")] = bounds.width;
        rect[QStringLiteral("height")] = bounds.height;
        node[QStringLiteral("bounds")] = rect;
    }

    *budget -= 1;
    QJsonArray children;
    if (depth < kMaxDepth && *budget > 0) {
        const QDBusMessage childrenReply = accessible.call(QStringLiteral("GetChildren"));
        if (childrenReply.type() == QDBusMessage::ReplyMessage &&
            !childrenReply.arguments().isEmpty()) {
            for (const ObjectRef& ref : readChildren(childrenReply.arguments().first())) {
                if (*budget <= 0) {
                    break;
                }
                if (ref.path == QLatin1String(kNullPath) || ref.name.isEmpty()) {
                    continue;
                }
                children.append(walk(bus, ref.name, ref.path, depth + 1, budget));
            }
        }
    }
    node[QStringLiteral("children")] = children;
    return node;
}

int subtreeSize(const QJsonObject& node) {
    int total = 1;
    for (const QJsonValue& child : node[QStringLiteral("children")].toArray()) {
        total += subtreeSize(child.toObject());
    }
    return total;
}

int subtreeDepth(const QJsonObject& node) {
    int deepest = 0;
    for (const QJsonValue& child : node[QStringLiteral("children")].toArray()) {
        deepest = qMax(deepest, subtreeDepth(child.toObject()));
    }
    return 1 + deepest;
}

/// Named objects strictly below the application's top-level windows. The
/// registry's process entry and a window's own title are identity of the
/// container, never of a target, so they must not count.
int namedBelow(const QJsonObject& node) {
    int total = 0;
    for (const QJsonValue& child : node[QStringLiteral("children")].toArray()) {
        const QJsonObject object = child.toObject();
        if (!object[QStringLiteral("name")].toString().isEmpty()) {
            total += 1;
        }
        total += namedBelow(object);
    }
    return total;
}

/// Whether two objects report the same rectangle. The rectangle is nested under
/// `bounds`, not spread across the object, so reading it from the top level
/// compares zeros and makes every object look identical to its window.
bool sameBounds(const QJsonObject& a, const QJsonObject& b) {
    const QJsonObject rectA = a[QStringLiteral("bounds")].toObject();
    const QJsonObject rectB = b[QStringLiteral("bounds")].toObject();
    return rectA[QStringLiteral("x")].toInt() == rectB[QStringLiteral("x")].toInt() &&
           rectA[QStringLiteral("y")].toInt() == rectB[QStringLiteral("y")].toInt() &&
           rectA[QStringLiteral("width")].toInt() == rectB[QStringLiteral("width")].toInt() &&
           rectA[QStringLiteral("height")].toInt() == rectB[QStringLiteral("height")].toInt();
}

/// Objects below a window whose rectangle differs from the window's: real
/// controls the agent can address, as opposed to a window-spanning filler.
int interiorBelow(const QJsonObject& node, const QJsonObject& window) {
    int total = 0;
    for (const QJsonValue& child : node[QStringLiteral("children")].toArray()) {
        const QJsonObject object = child.toObject();
        if (object.contains(QStringLiteral("bounds")) && !sameBounds(object, window)) {
            total += 1;
        }
        total += interiorBelow(object, window);
    }
    return total;
}

/// Measure what the application published, given the registry's root node.
QJsonObject computeStatsImpl(const QJsonObject& root) {
    const QJsonArray applications = root[QStringLiteral("children")].toArray();
    int nodes = 0;
    int named = 0;
    int interior = 0;
    int maxDepth = 0;
    for (const QJsonValue& appValue : applications) {
        const QJsonObject app = appValue.toObject();
        nodes += subtreeSize(app);
        maxDepth = qMax(maxDepth, subtreeDepth(app));
        for (const QJsonValue& windowValue : app[QStringLiteral("children")].toArray()) {
            const QJsonObject window = windowValue.toObject();
            named += namedBelow(window);
            interior += interiorBelow(window, window);
        }
    }
    QJsonObject stats;
    stats[QStringLiteral("applications")] = applications.size();
    stats[QStringLiteral("nodes")] = nodes;
    stats[QStringLiteral("named")] = named;
    stats[QStringLiteral("interior")] = interior;
    stats[QStringLiteral("max_depth")] = maxDepth;
    return stats;
}

} // namespace

bool Accessibility::awaitRegistry(const QString& busAddress, int timeoutMs) {
    QString error;
    const QString a11y = accessibilityBus(busAddress, &error);
    if (a11y.isEmpty()) {
        return false;
    }
    QDBusConnection connection = QDBusConnection::connectToBus(a11y, busNameFor(busAddress));
    if (!connection.isConnected()) {
        return false;
    }
    QDBusInterface root(kRegistryName, kRegistryRootPath,
                        QStringLiteral("org.a11y.atspi.Accessible"), connection);
    QElapsedTimer timer;
    timer.start();
    while (timer.elapsed() < timeoutMs) {
        if (root.call(QStringLiteral("GetChildCount")).type() == QDBusMessage::ReplyMessage) {
            return true;
        }
        QThread::msleep(100);
    }
    return false;
}

QJsonObject Accessibility::tree(const QString& busAddress, QString* error) {
    QString lookupError;
    const QString a11y = accessibilityBus(busAddress, &lookupError);
    if (a11y.isEmpty()) {
        if (error) {
            *error = lookupError;
        }
        return QJsonObject();
    }
    QDBusConnection connection = QDBusConnection::connectToBus(a11y, busNameFor(busAddress));
    if (!connection.isConnected()) {
        if (error) {
            *error = QStringLiteral("could not connect to the accessibility bus");
        }
        return QJsonObject();
    }

    int budget = kMaxNodes;
    const QJsonObject root = walk(connection, kRegistryName, kRegistryRootPath, 0, &budget);

    // The registry always exposes its own chrome: a desktop root plus one
    // application entry per publishing process. Everything the application
    // actually published lives inside its windows.
    const QJsonArray applications = root[QStringLiteral("children")].toArray();
    const QJsonObject stats = computeStatsImpl(root);

    if (applications.isEmpty() || stats[QStringLiteral("interior")].toInt() == 0) {
        if (error) {
            *error = QStringLiteral("no accessible objects inside its windows");
        }
        return QJsonObject();
    }

    QJsonObject result = root;
    result[QStringLiteral("stats")] = stats;
    return result;
}

QJsonObject Accessibility::computeStats(const QJsonObject& root) {
    return computeStatsImpl(root);
}

QJsonObject Accessibility::debugWalk(const QString& busAddress) {
    QString lookupError;
    const QString a11y = accessibilityBus(busAddress, &lookupError);
    QJsonObject result;
    result[QStringLiteral("a11yBus")] = a11y;
    if (a11y.isEmpty()) {
        result[QStringLiteral("error")] = lookupError;
        return result;
    }
    QDBusConnection connection = QDBusConnection::connectToBus(a11y, busNameFor(busAddress));
    result[QStringLiteral("connected")] = connection.isConnected();
    int budget = kMaxNodes;
    const QJsonObject root = walk(connection, kRegistryName, kRegistryRootPath, 0, &budget);
    result[QStringLiteral("root")] = root;
    result[QStringLiteral("stats")] = computeStatsImpl(root);
    return result;
}
