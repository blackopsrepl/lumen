#include "agentserver.h"

#include "accessibility.h"
#include "compositor.h"
#include "feedbackstore.h"
#include "session.h"
#include "sessionmanager.h"

#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLocalServer>
#include <QLocalSocket>
#include <QPointF>
#include <QSharedPointer>

AgentServer::AgentServer(SessionManager* sessions, FeedbackStore* feedback,
                         LumenCompositor* compositor, QObject* parent)
    : QObject(parent), m_sessions(sessions), m_feedback(feedback), m_compositor(compositor) {}

AgentServer::~AgentServer() {
    if (m_server) {
        m_server->close();
    }
    if (!m_path.isEmpty()) {
        QLocalServer::removeServer(m_path);
    }
}

bool AgentServer::listen(const QString& path) {
    // A socket left behind by a crashed instance would otherwise block the
    // bind forever; the lock is per-user, so removing it is safe.
    QLocalServer::removeServer(path);
    m_server = new QLocalServer(this);
    if (!m_server->listen(path)) {
        return false;
    }
    m_path = path;
    connect(m_server, &QLocalServer::newConnection, this, [this]() {
        while (QLocalSocket* socket = m_server->nextPendingConnection()) {
            handleConnection(socket);
        }
    });
    return true;
}

void AgentServer::handleConnection(QLocalSocket* socket) {
    connect(socket, &QLocalSocket::disconnected, socket, &QLocalSocket::deleteLater);
    // Requests are newline-delimited, and a client that sends more than one on a
    // single connection has them arrive in the same read. Parsing the whole read
    // as one document would see the concatenation and reject it as invalid JSON,
    // which silently turns every such request into an error.
    auto buffer = QSharedPointer<QByteArray>::create();
    connect(socket, &QLocalSocket::readyRead, this, [this, socket, buffer]() {
        *buffer += socket->readAll();
        int newline = buffer->indexOf('\n');
        while (newline >= 0) {
            const QByteArray line = buffer->left(newline).trimmed();
            buffer->remove(0, newline + 1);
            if (!line.isEmpty()) {
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
            }
            newline = buffer->indexOf('\n');
        }
    });
}

QJsonObject AgentServer::dispatch(const QJsonObject& request) {
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
        Session* session = m_sessions->session(name);
        if (!session) {
            response[QStringLiteral("error")] = QStringLiteral("no such session");
            return response;
        }
        return Accessibility::debugWalk(session->busAddress());
    }
    if (command == QLatin1String("accessibility")) {
        Session* session = m_sessions->session(name);
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
    if (command == QLatin1String("click") || command == QLatin1String("type")) {
        // Input that lands nowhere is an error, not `ok: false` on exit 0: an
        // agent reading only the exit status would otherwise carry on driving
        // a session that is showing nothing, and the human would watch a view
        // that never moves.
        Session* session = m_sessions->session(name);
        if (!session) {
            response[QStringLiteral("error")] = QStringLiteral("no such session '%1'").arg(name);
            return response;
        }
        if (!m_compositor->viewFor(name)) {
            response[QStringLiteral("error")] =
                QStringLiteral("session '%1' has no window on screen (state: %2); the "
                               "application has not mapped a surface, or has exited — see "
                               "its session.log")
                    .arg(name, session->state());
            return response;
        }
        if (command == QLatin1String("click")) {
            const QPointF point(request[QStringLiteral("x")].toDouble(),
                                request[QStringLiteral("y")].toDouble());
            if (!m_compositor->click(name, point)) {
                const QSize size = m_compositor->frame(name).size();
                response[QStringLiteral("error")] =
                    QStringLiteral("nothing drawn at (%1, %2) in session '%3'; its frame is %4x%5")
                        .arg(point.x())
                        .arg(point.y())
                        .arg(name)
                        .arg(size.width())
                        .arg(size.height());
                return response;
            }
        } else if (!m_compositor->type(name, request[QStringLiteral("text")].toString())) {
            response[QStringLiteral("error")] =
                QStringLiteral("session '%1' has no surface to type into").arg(name);
            return response;
        }
        response[QStringLiteral("ok")] = true;
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
    if (command == QLatin1String("add")) {
        // The human's note arrives from the viewer, carrying the annotated
        // region as a base64 PNG so the note still shows what was meant after
        // the surface has moved on.
        const QByteArray image =
            QByteArray::fromBase64(request[QStringLiteral("screenshot")].toString().toLatin1());
        const QJsonObject note =
            m_feedback->add(name, request[QStringLiteral("author")].toString(),
                            request[QStringLiteral("comment")].toString(), image);
        if (note.isEmpty()) {
            // Do not answer a failed save with a note object: the caller would
            // read it as success. Say what went wrong instead.
            response[QStringLiteral("error")] = m_feedback->lastError().isEmpty()
                                                    ? QStringLiteral("could not save the note")
                                                    : m_feedback->lastError();
            return response;
        }
        response[QStringLiteral("note")] = note;
        return response;
    }
    if (command == QLatin1String("note-image")) {
        // The screenshot a note carries, base64-encoded for the viewer.
        const QByteArray image =
            m_feedback->screenshot(name, request[QStringLiteral("id")].toInteger());
        response[QStringLiteral("image")] = QString::fromLatin1(image.toBase64());
        return response;
    }
    response[QStringLiteral("error")] = QStringLiteral("unknown command '%1'").arg(command);
    return response;
}
