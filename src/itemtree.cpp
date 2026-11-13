#include "itemtree.h"

#include <QDeadlineTimer>
#include <QDir>
#include <QJsonArray>
#include <QJsonDocument>
#include <QLocalSocket>

namespace {

QJsonObject ask(const QString& path, bool all, int timeoutMs) {
    QLocalSocket socket;
    socket.connectToServer(path);
    if (!socket.waitForConnected(timeoutMs)) {
        return QJsonObject();
    }
    const QJsonObject request{{QStringLiteral("cmd"), QStringLiteral("tree")},
                              {QStringLiteral("all"), all}};
    socket.write(QJsonDocument(request).toJson(QJsonDocument::Compact) + "\n");
    socket.flush();
    QByteArray reply;
    const QDeadlineTimer deadline(timeoutMs);
    while (!reply.contains('\n')) {
        if (deadline.hasExpired() || !socket.waitForReadyRead(int(deadline.remainingTime()))) {
            return QJsonObject();
        }
        reply += socket.readAll();
    }
    return QJsonDocument::fromJson(reply.left(reply.indexOf('\n'))).object();
}

QJsonObject translated(QJsonObject node, int dx, int dy) {
    QJsonArray rect = node.value(QStringLiteral("rect")).toArray();
    if (rect.size() == 4) {
        rect[0] = rect[0].toInt() + dx;
        rect[1] = rect[1].toInt() + dy;
        node[QStringLiteral("rect")] = rect;
    }
    const QJsonArray children = node.value(QStringLiteral("children")).toArray();
    if (!children.isEmpty()) {
        QJsonArray moved;
        for (const QJsonValue& child : children) {
            moved.append(translated(child.toObject(), dx, dy));
        }
        node[QStringLiteral("children")] = moved;
    }
    return node;
}

} // namespace

namespace {

QJsonObject movedBounds(QJsonObject node, int dx, int dy) {
    QJsonObject bounds = node.value(QStringLiteral("bounds")).toObject();
    // 0x0 bounds belong to controls that are not on screen; there is no
    // place to move them to.
    if (bounds.value(QStringLiteral("width")).toInt() > 0 ||
        bounds.value(QStringLiteral("height")).toInt() > 0) {
        bounds[QStringLiteral("x")] = bounds.value(QStringLiteral("x")).toInt() + dx;
        bounds[QStringLiteral("y")] = bounds.value(QStringLiteral("y")).toInt() + dy;
        node[QStringLiteral("bounds")] = bounds;
    }
    const QJsonArray children = node.value(QStringLiteral("children")).toArray();
    if (!children.isEmpty()) {
        QJsonArray moved;
        for (const QJsonValue& child : children) {
            moved.append(movedBounds(child.toObject(), dx, dy));
        }
        node[QStringLiteral("children")] = moved;
    }
    return node;
}

struct Origin {
    QSize size;
    QPoint origin;
    bool used = false;
};

/// Walk to each window-level node and move its subtree onto its placed origin.
QJsonObject placeFrames(QJsonObject node, QList<Origin>* origins, int* translated) {
    static const QStringList windowRoles{QStringLiteral("frame"), QStringLiteral("window"),
                                         QStringLiteral("dialog")};
    if (windowRoles.contains(node.value(QStringLiteral("role")).toString())) {
        const QJsonObject bounds = node.value(QStringLiteral("bounds")).toObject();
        const QSize size(bounds.value(QStringLiteral("width")).toInt(),
                         bounds.value(QStringLiteral("height")).toInt());
        for (Origin& origin : *origins) {
            if (!origin.used && origin.size == size && !size.isEmpty()) {
                origin.used = true;
                ++*translated;
                return movedBounds(node, origin.origin.x(), origin.origin.y());
            }
        }
        return node;
    }
    const QJsonArray children = node.value(QStringLiteral("children")).toArray();
    if (!children.isEmpty()) {
        QJsonArray placed;
        for (const QJsonValue& child : children) {
            placed.append(placeFrames(child.toObject(), origins, translated));
        }
        node[QStringLiteral("children")] = placed;
    }
    return node;
}

} // namespace

int ItemTree::translateAccessibility(QJsonObject* tree, const QJsonObject& placed) {
    QList<Origin> origins;
    for (const QJsonValue& value : placed.value(QStringLiteral("windows")).toArray()) {
        const QJsonObject window = value.toObject();
        if (!window.value(QStringLiteral("mapped")).toBool()) {
            continue;
        }
        const QJsonArray size = window.value(QStringLiteral("size")).toArray();
        const QJsonArray origin = window.value(QStringLiteral("origin")).toArray();
        origins.append({QSize(size.at(0).toInt(), size.at(1).toInt()),
                        QPoint(origin.at(0).toInt(), origin.at(1).toInt())});
    }
    int translated = 0;
    *tree = placeFrames(*tree, &origins, &translated);
    return translated;
}

QList<QJsonObject> ItemTree::query(const QString& probeDir, bool all, int timeoutMs) {
    QList<QJsonObject> replies;
    if (probeDir.isEmpty()) {
        return replies;
    }
    const QDir dir(probeDir);
    const QStringList sockets = dir.entryList({QStringLiteral("lumen-probe-*.sock")},
                                              QDir::System | QDir::Files, QDir::Name);
    for (const QString& name : sockets) {
        const QJsonObject reply = ask(dir.filePath(name), all, timeoutMs);
        if (reply.contains(QStringLiteral("windows"))) {
            replies.append(reply);
        }
    }
    return replies;
}

QJsonObject ItemTree::place(const QList<QJsonObject>& processes,
                            const QList<LumenCompositor::Placement>& placements) {
    QList<bool> used(placements.size(), false);
    QJsonArray windows;
    int hidden = 0;
    for (const QJsonObject& process : processes) {
        const qint64 pid = process.value(QStringLiteral("pid")).toInteger();
        hidden += process.value(QStringLiteral("hidden")).toInt();
        for (const QJsonValue& value : process.value(QStringLiteral("windows")).toArray()) {
            QJsonObject window = value.toObject();
            const QJsonArray size = window.value(QStringLiteral("size")).toArray();
            const QJsonArray margins = window.value(QStringLiteral("margins")).toArray();
            const int width = size.at(0).toInt();
            const int height = size.at(1).toInt();
            const int left = margins.at(0).toInt();
            const int top = margins.at(1).toInt();
            const QSize decorated(width + left + margins.at(2).toInt(),
                                  height + top + margins.at(3).toInt());
            // The buffer is the window plus its client-side decoration when
            // the client draws one, and exactly the window when it does not.
            int match = -1;
            QPoint content;
            for (int i = 0; i < placements.size() && match < 0; ++i) {
                if (used[i] || placements[i].pid != pid) {
                    continue;
                }
                if (placements[i].size == decorated) {
                    match = i;
                    content = placements[i].origin + QPoint(left, top);
                } else if (placements[i].size == QSize(width, height)) {
                    match = i;
                    content = placements[i].origin;
                }
            }
            window[QStringLiteral("pid")] = pid;
            window.remove(QStringLiteral("margins"));
            if (match < 0) {
                window[QStringLiteral("mapped")] = false;
            } else {
                used[match] = true;
                window[QStringLiteral("mapped")] = true;
                window[QStringLiteral("origin")] = QJsonArray{content.x(), content.y()};
                if (window.contains(QStringLiteral("root"))) {
                    window[QStringLiteral("root")] = translated(
                        window.value(QStringLiteral("root")).toObject(), content.x(), content.y());
                }
            }
            windows.append(window);
        }
    }
    return QJsonObject{{QStringLiteral("windows"), windows}, {QStringLiteral("hidden"), hidden}};
}
