#include "agentserver.h"

#include "accessibility.h"
#include "compositor.h"
#include "feedbackstore.h"
#include "session.h"
#include "sessionmanager.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QPointF>
#include <QLocalServer>
#include <QLocalSocket>

AgentServer::AgentServer(SessionManager *sessions, FeedbackStore *feedback,
                         LumenCompositor *compositor, QObject *parent)
    : QObject(parent), m_sessions(sessions), m_feedback(feedback), m_compositor(compositor) {}

AgentServer::~AgentServer() {
    if (m_server) {
        m_server->close();
    }
    if (!m_path.isEmpty()) {
        QLocalServer::removeServer(m_path);
    }
}

bool AgentServer::listen(const QString &path) {
    // A socket left behind by a crashed instance would otherwise block the
    // bind forever; the lock is per-user, so removing it is safe.
    QLocalServer::removeServer(path);
    m_server = new QLocalServer(this);
    if (!m_server->listen(path)) {
        return false;
    }
    m_path = path;
    connect(m_server, &QLocalServer::newConnection, this, [this]() {
        while (QLocalSocket *socket = m_server->nextPendingConnection()) {
            handleConnection(socket);
        }
    });
    return true;
}

void AgentServer::handleConnection(QLocalSocket *socket) {
    connect(socket, &QLocalSocket::disconnected, socket, &QLocalSocket::deleteLater);
    connect(socket, &QLocalSocket::readyRead, this, [this, socket]() {
        const QByteArray line = socket->readAll().trimmed();
        if (line.isEmpty()) {
            return;
        }
        QJsonParseError parseError;
        const QJsonDocument request = QJsonDocument::fromJson(line, &parseError);
        QJsonObject response;
        if (parseError.error != QJsonParseError::NoError) {
            response[QStringLiteral("error")] = QStringLiteral("invalid JSON request");
        } else {
            response = dispatch(request.object());
        }
        socket->write(QJsonDocument(response).toJson(QJsonDocument::Compact) + "\n");
        socket->flush();
        socket->disconnectFromServer();
    });
}

QJsonObject AgentServer::dispatch(const QJsonObject &request) {
    const QString command = request[QStringLiteral("cmd")].toString();
    const QString name = request[QStringLiteral("name")].toString();
    QJsonObject response;

    if (command == QLatin1String("status")) {
        response[QStringLiteral("sessions")] = QJsonArray::fromVariantList(m_sessions->sessions());
        return response;
    }
    if (command == QLatin1String("ensure")) {
        QString error;
        const QString created =
            m_sessions->create(name, request[QStringLiteral("command")].toString(), true,
                               request[QStringLiteral("owner")].toString(), &error);
        if (created.isEmpty()) {
            response[QStringLiteral("error")] = error;
            return response;
        }
        response[QStringLiteral("ok")] = true;
        response[QStringLiteral("name")] = created;
        return response;
    }
    if (command == QLatin1String("stop")) {
        response[QStringLiteral("ok")] = m_sessions->stop(name);
        return response;
    }
    if (command == QLatin1String("accessibility-debug")) {
        Session *session = m_sessions->session(name);
        if (!session) {
            response[QStringLiteral("error")] = QStringLiteral("no such session");
            return response;
        }
        return Accessibility::debugWalk(session->busAddress());
    }
    if (command == QLatin1String("accessibility")) {
        Session *session = m_sessions->session(name);
        if (!session) {
            response[QStringLiteral("error")] = QStringLiteral("no such session");
            return response;
        }
        if (session->busAddress().isEmpty()) {
            response[QStringLiteral("error")] =
                QStringLiteral("session does not publish an accessibility tree");
            return response;
        }
        QString error;
        const QJsonObject tree = Accessibility::tree(session->busAddress(), &error);
        if (tree.isEmpty()) {
            response[QStringLiteral("error")] = error;
            return response;
        }
        return tree;
    }
    if (command == QLatin1String("click")) {
        response[QStringLiteral("ok")] =
            m_compositor->click(name, QPointF(request[QStringLiteral("x")].toDouble(),
                                              request[QStringLiteral("y")].toDouble()));
        return response;
    }
    if (command == QLatin1String("type")) {
        response[QStringLiteral("ok")] =
            m_compositor->type(name, request[QStringLiteral("text")].toString());
        return response;
    }
    if (command == QLatin1String("feedback")) {
        const bool consume = request[QStringLiteral("consume")].toBool();
        response[QStringLiteral("notes")] =
            consume ? m_feedback->consume(name) : m_feedback->list(name, true);
        return response;
    }
    if (command == QLatin1String("ack")) {
        response[QStringLiteral("ok")] =
            m_feedback->ack(name, request[QStringLiteral("id")].toInteger());
        return response;
    }
    response[QStringLiteral("error")] =
        QStringLiteral("unknown command '%1'").arg(command);
    return response;
}
