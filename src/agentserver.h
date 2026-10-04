// The agent-facing surface: a unix socket speaking JSON lines.
//
// Lumen has no network surface. An agent reaches it over a local socket in the
// user's runtime directory, which is the whole transport — no HTTP server, no
// port, nothing reachable off the machine.
//
// Every input command is dispatched to the compositor, which is the single place
// a click or a key is delivered. The viewer's human input takes the same path, so
// an agent and a human can never diverge.

#pragma once

#include <QObject>
#include <QString>

class FeedbackStore;
class LumenCompositor;
class QLocalServer;
class QLocalSocket;
class SessionManager;

class AgentServer : public QObject {
    Q_OBJECT

  public:
    AgentServer(SessionManager* sessions, FeedbackStore* feedback, LumenCompositor* compositor,
                QObject* parent = nullptr);
    ~AgentServer() override;

    /// Listen on `path`. Returns false when another instance owns it.
    bool listen(const QString& path);
    QString path() const { return m_path; }

  private:
    void handleConnection(QLocalSocket* socket);
    QJsonObject dispatch(const QJsonObject& request);

    SessionManager* m_sessions;
    FeedbackStore* m_feedback;
    LumenCompositor* m_compositor;
    QLocalServer* m_server = nullptr;
    QString m_path;
};
