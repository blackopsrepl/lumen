#include "framestream.h"

#include "compositor.h"

#include <QBuffer>
#include <QImage>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLocalServer>
#include <QLocalSocket>
#include <QTimer>

namespace {

/// How often the active session's frame is sampled for viewers.
///
/// The compositor hands back the client's current buffer, so sampling is cheap;
/// this bounds how often a frame is encoded and sent.
constexpr int kFrameIntervalMs = 66;

/// JPEG quality for streamed frames.
constexpr int kQuality = 80;

} // namespace

FrameStream::FrameStream(LumenCompositor *compositor, QObject *parent)
    : QObject(parent), m_compositor(compositor) {}

FrameStream::~FrameStream() {
    if (m_server) {
        m_server->close();
    }
    if (!m_path.isEmpty()) {
        QLocalServer::removeServer(m_path);
    }
}

bool FrameStream::listen(const QString &path) {
    QLocalServer::removeServer(path);
    m_server = new QLocalServer(this);
    if (!m_server->listen(path)) {
        return false;
    }
    m_path = path;
    connect(m_server, &QLocalServer::newConnection, this, [this]() {
        while (QLocalSocket *viewer = m_server->nextPendingConnection()) {
            connect(viewer, &QLocalSocket::disconnected, this, [this, viewer]() {
                m_subscriptions.remove(viewer);
                m_lastSession.remove(viewer);
                viewer->deleteLater();
            });
            connect(viewer, &QLocalSocket::readyRead, this, [this, viewer]() {
                const QJsonObject request =
                    QJsonDocument::fromJson(viewer->readAll().trimmed()).object();
                subscribe(viewer, request[QStringLiteral("session")].toString());
            });
        }
    });

    auto *pump = new QTimer(this);
    connect(pump, &QTimer::timeout, this, &FrameStream::pump);
    pump->start(kFrameIntervalMs);
    return true;
}

void FrameStream::subscribe(QLocalSocket *viewer, const QString &session) {
    m_subscriptions.insert(viewer, session);
    m_lastSession.insert(viewer, QString());
}

void FrameStream::pump() {
    if (m_subscriptions.isEmpty()) {
        return;
    }
    for (auto it = m_subscriptions.constBegin(); it != m_subscriptions.constEnd(); ++it) {
        QLocalSocket *viewer = it.key();
        const QString session = it.value();
        if (session.isEmpty() || viewer->state() != QLocalSocket::ConnectedState) {
            continue;
        }
        const QImage frame = m_compositor->frame(session);
        if (frame.isNull()) {
            continue;
        }
        QByteArray jpeg;
        QBuffer buffer(&jpeg);
        buffer.open(QIODevice::WriteOnly);
        frame.save(&buffer, "JPEG", kQuality);

        // A frame is length-prefixed with a 4-byte big-endian size, so the
        // viewer can find frame boundaries on a byte stream.
        const quint32 length = jpeg.size();
        QByteArray header;
        header.append(char((length >> 24) & 0xFF));
        header.append(char((length >> 16) & 0xFF));
        header.append(char((length >> 8) & 0xFF));
        header.append(char(length & 0xFF));
        viewer->write(header + jpeg);
        viewer->flush();
    }
}
