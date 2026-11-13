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
