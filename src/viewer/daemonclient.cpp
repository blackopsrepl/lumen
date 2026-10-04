#include "daemonclient.h"

#include <QBuffer>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLocalSocket>
#include <QStandardPaths>
#include <QTimer>
#include <QtGlobal>

namespace {

/// The socket the daemon listens on, in the user's runtime directory. The viewer
/// derives it the same way the daemon does, so neither has to be configured.
QString runtimeSocket(const QString& leaf) {
    QString runtime = QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
    if (runtime.isEmpty()) {
        runtime = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);
    }
    return runtime + QLatin1Char('/') + leaf;
}

/// Honour an explicit path when one is published.
///
/// A viewer running inside a Lumen session has its own XDG_RUNTIME_DIR, so it
/// cannot derive the daemon's socket location; the daemon exports it instead.
QString publishedSocket(const char* envVar, const QString& fallback) {
    const QByteArray published = qgetenv(envVar);
    return published.isEmpty() ? fallback : QString::fromLocal8Bit(published);
}

} // namespace

QString DaemonClient::controlSocketPath() {
    return publishedSocket("LUMEN_AGENT_SOCKET", runtimeSocket(QStringLiteral("lumen-agent.sock")));
}

QString DaemonClient::streamSocketPath() {
    return publishedSocket("LUMEN_STREAM_SOCKET",
                           runtimeSocket(QStringLiteral("lumen-stream.sock")));
}

DaemonClient::DaemonClient(QObject* parent) : QObject(parent) {
    m_control = new QLocalSocket(this);
    connect(m_control, &QLocalSocket::connected, this, [this]() {
        m_connected = true;
        emit connectedChanged();
        refresh();
    });
    connect(m_control, &QLocalSocket::disconnected, this, [this]() {
        m_connected = false;
        emit connectedChanged();
    });
    connect(m_control, &QLocalSocket::errorOccurred, this, [this](QLocalSocket::LocalSocketError) {
        m_connected = false;
        emit connectedChanged();
    });
    connect(m_control, &QLocalSocket::readyRead, this, [this]() {
        m_controlBuffer += m_control->readAll();
        // Responses are newline-delimited and a burst of them can arrive
        // together, so each complete line is handled on its own.
        int newline = m_controlBuffer.indexOf('\n');
        while (newline >= 0) {
            const QByteArray line = m_controlBuffer.left(newline).trimmed();
            m_controlBuffer.remove(0, newline + 1);
            if (!line.isEmpty()) {
                handleResponse(QJsonDocument::fromJson(line).object());
            }
            newline = m_controlBuffer.indexOf('\n');
        }
    });

    // Connect immediately, and keep refreshing: the daemon may not be running
    // yet when the viewer starts, and a session's state changes on its own
    // (an agent starts one, a client exits). Nothing else reconnects, so a
    // viewer that never dials here would show an empty list forever.
    QTimer* poll = new QTimer(this);
    poll->setInterval(1000);
    connect(poll, &QTimer::timeout, this, &DaemonClient::refresh);
    poll->start();
    refresh();
}

void DaemonClient::handleResponse(const QJsonObject& response) {
    if (response.contains(QStringLiteral("sessions"))) {
        m_sessions = response[QStringLiteral("sessions")].toArray().toVariantList();
        emit sessionsChanged();
    }
    if (response.contains(QStringLiteral("notes"))) {
        // The reply to a feedback request; it belongs to the session that asked,
        // which may not be the one selected now.
        const QString session = m_notesSession.isEmpty() ? m_activeName : m_notesSession;
        m_notes.insert(session, response[QStringLiteral("notes")].toArray().toVariantList());
        emit notesChanged(session);
    }
    if (response.contains(QStringLiteral("note"))) {
        // A note the human just added is echoed back, so the panel can show it
        // without waiting a full poll interval.
        const QVariantMap note = response[QStringLiteral("note")].toObject().toVariantMap();
        QVariantList notes = m_notes.value(m_activeName);
        notes.append(note);
        m_notes.insert(m_activeName, notes);
        emit notesChanged(m_activeName);
    }
}

void DaemonClient::send(const QByteArray& request) {
    if (m_control->state() != QLocalSocket::ConnectedState) {
        m_control->connectToServer(controlSocketPath());
        if (!m_control->waitForConnected(1000)) {
            qWarning("lumen: viewer cannot reach %s: %s", qPrintable(controlSocketPath()),
                     qPrintable(m_control->errorString()));
            return;
        }
    }
    m_control->write(request + "\n");
    m_control->flush();
}

void DaemonClient::refresh() {
    send(QJsonDocument(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("status")}})
             .toJson(QJsonDocument::Compact));
}

void DaemonClient::setActiveName(const QString& name) {
    if (m_activeName == name) {
        return;
    }
    m_activeName = name;
    emit activeNameChanged();
    subscribe(name);
}

void DaemonClient::subscribe(const QString& session) {
    if (m_stream) {
        m_stream->disconnectFromServer();
        m_stream->deleteLater();
        m_stream = nullptr;
    }
    m_streamBuffer.clear();
    if (session.isEmpty()) {
        return;
    }
    m_stream = new QLocalSocket(this);
    connect(m_stream, &QLocalSocket::readyRead, this, &DaemonClient::onStreamData);
    m_stream->connectToServer(streamSocketPath());
    if (!m_stream->waitForConnected(1000)) {
        return;
    }
    // The first line is the subscription; the rest of the socket is frames.
    m_stream->write(QJsonDocument(QJsonObject{{QStringLiteral("session"), session}})
                        .toJson(QJsonDocument::Compact) +
                    "\n");
    m_stream->flush();
}

void DaemonClient::onStreamData() {
    m_streamBuffer += m_stream->readAll();
    // Frames are length-prefixed: a 4-byte big-endian size, then a JPEG.
    while (m_streamBuffer.size() >= 4) {
        const quint32 length = (quint8(m_streamBuffer[0]) << 24) |
                               (quint8(m_streamBuffer[1]) << 16) |
                               (quint8(m_streamBuffer[2]) << 8) | quint8(m_streamBuffer[3]);
        if (m_streamBuffer.size() < int(4 + length)) {
            return;
        }
        const QByteArray jpeg = m_streamBuffer.mid(4, length);
        m_streamBuffer.remove(0, 4 + length);
        QImage image;
        if (image.loadFromData(jpeg, "JPEG")) {
            m_frame = image;
            // The URL carries a counter so QML treats each frame as a new
            // image; an unchanging URL would be served from cache and the view
            // would sit on the first frame forever.
            m_frameUrl = QStringLiteral("image://lumen/frame/%1").arg(++m_frameSerial);
            emit frameChanged();
        }
    }
}

bool DaemonClient::click(qreal x, qreal y) {
    if (m_activeName.isEmpty()) {
        return false;
    }
    send(QJsonDocument(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("click")},
                                   {QStringLiteral("name"), m_activeName},
                                   {QStringLiteral("x"), x},
                                   {QStringLiteral("y"), y}})
             .toJson(QJsonDocument::Compact));
    return true;
}

bool DaemonClient::type(const QString& text) {
    if (m_activeName.isEmpty()) {
        return false;
    }
    send(QJsonDocument(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("type")},
                                   {QStringLiteral("name"), m_activeName},
                                   {QStringLiteral("text"), text}})
             .toJson(QJsonDocument::Compact));
    return true;
}

QString DaemonClient::accessibility() {
    if (m_activeName.isEmpty()) {
        return QString();
    }
    send(QJsonDocument(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("accessibility")},
                                   {QStringLiteral("name"), m_activeName}})
             .toJson(QJsonDocument::Compact));
    return QString();
}

void DaemonClient::stopSession(const QString& name) {
    send(QJsonDocument(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("stop")},
                                   {QStringLiteral("name"), name}})
             .toJson(QJsonDocument::Compact));
}

QVariantList DaemonClient::notes(const QString& session) {
    return m_notes.value(session);
}

void DaemonClient::refreshNotes(const QString& session) {
    if (session.isEmpty()) {
        return;
    }
    // The reply is matched to the session that asked, so the active session is
    // tracked while the request is in flight.
    m_notesSession = session;
    send(QJsonDocument(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("feedback")},
                                   {QStringLiteral("name"), session},
                                   {QStringLiteral("consume"), false}})
             .toJson(QJsonDocument::Compact));
}

void DaemonClient::addNote(const QString& session, const QString& comment, int x, int y, int w,
                           int h) {
    if (session.isEmpty() || comment.trimmed().isEmpty()) {
        return;
    }
    // The annotated region is cropped here, at send time, so the note carries the
    // pixels the human was looking at rather than coordinates a redraw would move
    // out from under it. The crop lives in C++ because QML cannot build a QImage:
    // `QImage::copy` is not invokable.
    QByteArray png;
    if (w > 0 && h > 0 && !m_frame.isNull()) {
        // Clamp to the frame: a selection that runs past the edge would
        // otherwise produce an empty or malformed crop.
        const QRect region = QRect(x, y, w, h).intersected(QRect(QPoint(0, 0), m_frame.size()));
        if (region.width() > 0 && region.height() > 0) {
            QBuffer buffer(&png);
            buffer.open(QIODevice::WriteOnly);
            m_frame.copy(region).save(&buffer, "PNG");
        }
    }
    send(QJsonDocument(
             QJsonObject{{QStringLiteral("cmd"), QStringLiteral("add")},
                         {QStringLiteral("name"), session},
                         {QStringLiteral("author"), QStringLiteral("human")},
                         {QStringLiteral("comment"), comment},
                         {QStringLiteral("screenshot"), QString::fromLatin1(png.toBase64())}})
             .toJson(QJsonDocument::Compact));
}

void DaemonClient::resolveNote(const QString& session, int id) {
    send(QJsonDocument(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("ack")},
                                   {QStringLiteral("name"), session},
                                   {QStringLiteral("id"), id}})
             .toJson(QJsonDocument::Compact));
}

QString DaemonClient::noteImage(const QString& session, int id) {
    // Notes carry their pixels in the daemon's store; the viewer fetches them on
    // demand rather than holding every image in memory.
    QLocalSocket socket;
    socket.connectToServer(controlSocketPath());
    if (!socket.waitForConnected(1000)) {
        return QString();
    }
    socket.write(QJsonDocument(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("note-image")},
                                           {QStringLiteral("name"), session},
                                           {QStringLiteral("id"), id}})
                     .toJson(QJsonDocument::Compact) +
                 "\n");
    socket.flush();
    if (!socket.waitForReadyRead(3000)) {
        return QString();
    }
    return QJsonDocument::fromJson(socket.readAll().trimmed())
        .object()[QStringLiteral("image")]
        .toString();
}
