// lumenprobe — the in-process half of `lumen-cli tree`.
//
// A Qt generic plugin that Lumen loads into every session's application through
// QT_QPA_GENERIC_PLUGINS. The application is not modified and knows nothing
// about it. Inside the process the probe can see what no outside observer can:
// every window's complete item tree — plain Rectangles and unnamed Text
// included, which the accessibility tree omits — with each item's real scene
// geometry, visibility, clipping and the properties that decide how it looks.
//
// It answers one request on a unix socket in the directory the daemon names in
// LUMEN_PROBE_DIR, `lumen-probe-<pid>.sock`: a JSON line `{"cmd":"tree","all":bool}`
// gets the tree back as one JSON line. The daemon finds every probe of a
// session by globbing that directory and maps window coordinates to frame
// coordinates, because only the compositor knows where a surface sits.

#include <QApplication>
#include <QColor>
#include <QCoreApplication>
#include <QFile>
#include <QFont>
#include <QGenericPlugin>
#include <QGuiApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLocalServer>
#include <QLocalSocket>
#include <QMargins>
#include <QMetaProperty>
#include <QPointer>
#include <QQmlContext>
#include <QQmlEngine>
#include <QQuickItem>
#include <QQuickWindow>
#include <QRegularExpression>
#include <QSharedPointer>
#include <QUrl>
#include <QWidget>
#include <QWindow>

namespace {

/// The properties worth reporting when an item has them: what a control shows
/// and the inputs that decide whether its content fits its box.
const char* const kProperties[] = {
    "text",         "title",         "placeholderText", "checked",        "color",
    "source",       "font",          "elide",           "wrapMode",       "truncated",
    "contentWidth", "contentHeight", "implicitWidth",   "implicitHeight", "currentIndex",
    "count",        "radius",        "spacing",         "padding",
};

/// A readable type name: `Rectangle` for QQuickRectangle, `Card` for a QML
/// component's generated `Card_QMLTYPE_12`.
QString typeName(const QObject* object) {
    QString name = QString::fromLatin1(object->metaObject()->className());
    static const QRegularExpression generated(QStringLiteral("_QML(TYPE)?_[0-9]+$"));
    name.remove(generated);
    if (name.startsWith(QLatin1String("QQuick")) && name.size() > 6) {
        name = name.mid(6);
    }
    return name;
}

/// The QML id an object was declared with, from the context it was created in
/// or the component context enclosing it.
QString qmlId(QObject* object) {
    for (QQmlContext* context = qmlContext(object); context; context = context->parentContext()) {
        const QString id = context->nameForObject(object);
        if (!id.isEmpty()) {
            return id;
        }
    }
    return QString();
}

QJsonValue toJson(const QVariant& value) {
    switch (value.metaType().id()) {
    case QMetaType::QColor:
        return value.value<QColor>().name(QColor::HexArgb);
    case QMetaType::QFont: {
        const QFont font = value.value<QFont>();
        QJsonObject object{{QStringLiteral("family"), font.family()},
                           {QStringLiteral("bold"), font.bold()},
                           {QStringLiteral("weight"), int(font.weight())}};
        if (font.pixelSize() > 0) {
            object[QStringLiteral("pixelSize")] = font.pixelSize();
        } else {
            object[QStringLiteral("pointSize")] = font.pointSizeF();
        }
        return object;
    }
    case QMetaType::QUrl:
        return value.toUrl().toString();
    case QMetaType::Bool:
        return value.toBool();
    case QMetaType::QString:
        return value.toString();
    case QMetaType::Int:
    case QMetaType::UInt:
    case QMetaType::LongLong:
    case QMetaType::ULongLong:
        return value.toLongLong();
    case QMetaType::Double:
    case QMetaType::Float:
        return value.toDouble();
    default:
        // Enums arrive as their own metatype and convert to their value.
        if (value.metaType().flags() & QMetaType::IsEnumeration) {
            return value.toInt();
        }
        return QJsonValue();
    }
}

QJsonObject properties(const QObject* object) {
    QJsonObject result;
    const QMetaObject* meta = object->metaObject();
    for (const char* name : kProperties) {
        const int index = meta->indexOfProperty(name);
        if (index < 0) {
            continue;
        }
        const QJsonValue value = toJson(meta->property(index).read(object));
        if (!value.isUndefined() && !value.isNull()) {
            result[QLatin1String(name)] = value;
        }
    }
    return result;
}

QJsonArray rectArray(const QRectF& rect) {
    return QJsonArray{qRound(rect.x()), qRound(rect.y()), qRound(rect.width()),
                      qRound(rect.height())};
}

/// Walk one Quick item. `clip` is the scene rectangle the item's clipping
/// ancestors (and the window) leave visible; `clipOwner` names the nearest one.
QJsonObject walkItem(QQuickItem* item, const QRectF& clip, const QString& clipOwner, bool all,
                     int* hidden) {
    const QRectF rect = item->mapRectToScene(QRectF(0, 0, item->width(), item->height()));
    QJsonObject node{{QStringLiteral("type"), typeName(item)},
                     {QStringLiteral("rect"), rectArray(rect)}};
    const QString id = qmlId(item);
    if (!id.isEmpty()) {
        node[QStringLiteral("id")] = id;
    }
    if (!item->objectName().isEmpty()) {
        node[QStringLiteral("objectName")] = item->objectName();
    }
    if (!item->isVisible()) {
        node[QStringLiteral("visible")] = false;
    }
    if (item->opacity() < 1.0) {
        node[QStringLiteral("opacity")] = item->opacity();
    }
    if (item->z() != 0) {
        node[QStringLiteral("z")] = item->z();
    }
    if (item->clip()) {
        node[QStringLiteral("clip")] = true;
    }
    if (!item->isEnabled()) {
        node[QStringLiteral("enabled")] = false;
    }
    const QJsonObject props = properties(item);
    if (!props.isEmpty()) {
        node[QStringLiteral("props")] = props;
    }

    // What an agent would otherwise have to measure off pixels.
    if (rect.width() > 0 && rect.height() > 0 && !clip.contains(rect)) {
        node[QStringLiteral("clipped")] = clipOwner;
    }
    if (props.contains(QStringLiteral("contentWidth")) && props.contains(QStringLiteral("text"))) {
        const bool elided = props.value(QStringLiteral("truncated")).toBool();
        const double over = props.value(QStringLiteral("contentWidth")).toDouble() - item->width();
        const double overDown =
            props.value(QStringLiteral("contentHeight")).toDouble() - item->height();
        if (elided) {
            node[QStringLiteral("elided")] = true;
        } else if (over > 0.5 || overDown > 0.5) {
            node[QStringLiteral("overflow")] =
                QJsonArray{qRound(qMax(0.0, over)), qRound(qMax(0.0, overDown))};
        }
    }

    QRectF childClip = clip;
    QString childClipOwner = clipOwner;
    if (item->clip()) {
        childClip = clip.intersected(rect);
        childClipOwner = id.isEmpty() ? typeName(item) : id;
    }
    QJsonArray children;
    for (QQuickItem* child : item->childItems()) {
        if (!all && !child->isVisible()) {
            ++*hidden;
            continue;
        }
        children.append(walkItem(child, childClip, childClipOwner, all, hidden));
    }
    if (!children.isEmpty()) {
        node[QStringLiteral("children")] = children;
    }
    return node;
}

QJsonObject walkWidget(QWidget* widget, QWidget* top, bool all, int* hidden) {
    const QPoint origin = widget->mapTo(top, QPoint(0, 0));
    QJsonObject node{{QStringLiteral("type"), typeName(widget)},
                     {QStringLiteral("rect"),
                      rectArray(QRectF(origin, QSizeF(widget->width(), widget->height())))}};
    if (!widget->objectName().isEmpty()) {
        node[QStringLiteral("objectName")] = widget->objectName();
    }
    if (!widget->isVisible()) {
        node[QStringLiteral("visible")] = false;
    }
    if (!widget->isEnabled()) {
        node[QStringLiteral("enabled")] = false;
    }
    const QJsonObject props = properties(widget);
    if (!props.isEmpty()) {
        node[QStringLiteral("props")] = props;
    }
    QJsonArray children;
    for (QObject* object : widget->children()) {
        auto* child = qobject_cast<QWidget*>(object);
        if (!child || child->isWindow()) {
            continue;
        }
        if (!all && !child->isVisible()) {
            ++*hidden;
            continue;
        }
        children.append(walkWidget(child, top, all, hidden));
    }
    if (!children.isEmpty()) {
        node[QStringLiteral("children")] = children;
    }
    return node;
}

QJsonObject tree(bool all) {
    QJsonArray windows;
    int hidden = 0;
    for (QWindow* window : QGuiApplication::topLevelWindows()) {
        if (!all && !window->isVisible()) {
            continue;
        }
        const QMargins margins = window->frameMargins();
        QJsonObject node{{QStringLiteral("type"), typeName(window)},
                         {QStringLiteral("title"), window->title()},
                         {QStringLiteral("size"), QJsonArray{window->width(), window->height()}},
                         {QStringLiteral("margins"), QJsonArray{margins.left(), margins.top(),
                                                                margins.right(), margins.bottom()}},
                         {QStringLiteral("visible"), window->isVisible()}};
        const QRectF bounds(0, 0, window->width(), window->height());
        if (auto* quick = qobject_cast<QQuickWindow*>(window)) {
            node[QStringLiteral("root")] =
                walkItem(quick->contentItem(), bounds, QStringLiteral("window"), all, &hidden);
        } else if (qobject_cast<QApplication*>(QCoreApplication::instance())) {
            for (QWidget* widget : QApplication::topLevelWidgets()) {
                if (widget->windowHandle() == window) {
                    node[QStringLiteral("root")] = walkWidget(widget, widget, all, &hidden);
                    break;
                }
            }
        }
        windows.append(node);
    }
    return QJsonObject{{QStringLiteral("pid"), QCoreApplication::applicationPid()},
                       {QStringLiteral("windows"), windows},
                       {QStringLiteral("hidden"), hidden}};
}

class Probe : public QObject {
  public:
    explicit Probe(const QString& path) : m_path(path) {
        QLocalServer::removeServer(path);
        m_server = new QLocalServer(this);
        m_server->setSocketOptions(QLocalServer::UserAccessOption);
        if (!m_server->listen(path)) {
            qWarning("lumenprobe: cannot listen on %s: %s", qPrintable(path),
                     qPrintable(m_server->errorString()));
            return;
        }
        connect(m_server, &QLocalServer::newConnection, this, [this]() {
            while (QLocalSocket* socket = m_server->nextPendingConnection()) {
                serve(socket);
            }
        });
    }
    ~Probe() override { QLocalServer::removeServer(m_path); }

  private:
    void serve(QLocalSocket* socket) {
        connect(socket, &QLocalSocket::disconnected, socket, &QLocalSocket::deleteLater);
        auto buffer = QSharedPointer<QByteArray>::create();
        connect(socket, &QLocalSocket::readyRead, socket, [socket, buffer]() {
            *buffer += socket->readAll();
            const int newline = buffer->indexOf('\n');
            if (newline < 0) {
                return;
            }
            const QJsonObject request = QJsonDocument::fromJson(buffer->left(newline)).object();
            buffer->clear();
            QJsonObject reply;
            if (request[QStringLiteral("cmd")].toString() == QLatin1String("tree")) {
                reply = tree(request[QStringLiteral("all")].toBool());
            } else {
                reply[QStringLiteral("error")] = QStringLiteral("unknown probe request");
            }
            socket->write(QJsonDocument(reply).toJson(QJsonDocument::Compact) + "\n");
            socket->flush();
        });
    }

    QString m_path;
    QLocalServer* m_server = nullptr;
};

} // namespace

class LumenProbePlugin : public QGenericPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID QGenericPluginFactoryInterface_iid FILE "lumenprobe.json")

  public:
    QObject* create(const QString& key, const QString&) override {
        if (key.compare(QLatin1String("lumenprobe"), Qt::CaseInsensitive) != 0) {
            return nullptr;
        }
        // Only inside a Lumen session, which names the directory to listen in.
        const QString dir = qEnvironmentVariable("LUMEN_PROBE_DIR");
        if (qEnvironmentVariableIsEmpty("LUMEN_SESSION") || dir.isEmpty()) {
            return nullptr;
        }
        return new Probe(
            dir + QStringLiteral("/lumen-probe-%1.sock").arg(QCoreApplication::applicationPid()));
    }
};

#include "lumenprobe.moc"
