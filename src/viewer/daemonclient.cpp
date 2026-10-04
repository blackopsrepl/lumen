#include "daemonclient.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLocalSocket>
#include <QStandardPaths>

namespace {

/// The socket the daemon listens on, in the user's runtime directory. The viewer
/// derives it the same way the daemon does, so neither has to be configured.
QString runtimeSocket(const QString &leaf) {
    QString runtime = QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
    if (runtime.isEmpty()) {
        runtime = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);
    }
    return runtime + QLatin1Char('/') + leaf;
}

} // namespace

QString DaemonClient::controlSocketPath() { return runtimeSocket(QStringLiteral("lumen-agent.sock")); }
QString DaemonClient::streamSocketPath() { return runtimeSocket(QStringLiteral("lumen-stream.sock")); }

DaemonClient::DaemonClient(QObject *parent) : QObject(parent) {
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
        const QJsonObject response =
            QJsonDocument::fromJson(m_control->readAll().trimmed()).object();
        if (response.contains(QStringLiteral("sessions"))) {
            m_sessions = response[QStringLiteral("sessions")].toArray().toVariantList();
            emit sessionsChanged();
        }
    });
}

void DaemonClient::send(const QByteArray &request) {
    if (m_control->state() != QLocalSocket::ConnectedState) {
        m_control->connectToServer(controlSocketPath());
        if (!m_control->waitForConnected(1000)) {
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

void DaemonClient::setActiveName(const QString &name) {
    if (m_activeName == name) {
        return;
    }
    m_activeName = name;
    emit activeNameChanged();
    subscribe(name);
}

void DaemonClient::subscribe(const QString &session) {
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
                        .toJson(QJsonDocument::Compact) + "\n");
    m_stream->flush();
}

void DaemonClient::onStreamData() {
    m_streamBuffer += m_stream->readAll();
    // Frames are length-prefixed: a 4-byte big-endian size, then a JPEG.
    while (m_streamBuffer.size() >= 4) {
        const quint32 length =
            (quint8(m_streamBuffer[0]) << 24) | (quint8(m_streamBuffer[1]) << 16)
            | (quint8(m_streamBuffer[2]) << 8) | quint8(m_streamBuffer[3]);
        if (m_streamBuffer.size() < int(4 + length)) {
            return;
        }
        const QByteArray jpeg = m_streamBuffer.mid(4, length);
        m_streamBuffer.remove(0, 4 + length);
        QImage image;
        if (image.loadFromData(jpeg, "JPEG")) {
            m_frame = image;
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

bool DaemonClient::type(const QString &text) {
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

void DaemonClient::createSession(const QString &name, const QString &command,
                                 bool agentOwned, const QString &owner) {
    send(QJsonDocument(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("ensure")},
                                   {QStringLiteral("name"), name},
                                   {QStringLiteral("command"), command},
                                   {QStringLiteral("owner"), owner}})
             .toJson(QJsonDocument::Compact));
}

void DaemonClient::stopSession(const QString &name) {
    send(QJsonDocument(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("stop")},
                                   {QStringLiteral("name"), name}})
             .toJson(QJsonDocument::Compact));
}
