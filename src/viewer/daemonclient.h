// The viewer's link to the daemon.
//
// The viewer owns no sessions and no compositor. It asks the daemon for the
// session list, receives the active session's frames, and forwards human input.
// That is the whole relationship — which is what lets the viewer be closed while
// sessions keep running.

#pragma once

#include <QByteArray>
#include <QHash>
#include <QImage>
#include <QObject>
#include <QString>
#include <QVariantList>

class QLocalSocket;

class DaemonClient : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool connected READ connected NOTIFY connectedChanged)
    Q_PROPERTY(QVariantList sessions READ sessions NOTIFY sessionsChanged)
    Q_PROPERTY(QString activeName READ activeName WRITE setActiveName NOTIFY activeNameChanged)
    Q_PROPERTY(QImage frame READ frame NOTIFY frameChanged)

public:
    explicit DaemonClient(QObject *parent = nullptr);

    bool connected() const { return m_connected; }
    QVariantList sessions() const { return m_sessions; }
    QString activeName() const { return m_activeName; }
    void setActiveName(const QString &name);
    QImage frame() const { return m_frame; }

    /// Reconnect and refresh the session list.
    Q_INVOKABLE void refresh();
    /// Deliver a click at a point in the active surface's coordinates.
    Q_INVOKABLE bool click(qreal x, qreal y);
    /// Deliver text to the active session.
    Q_INVOKABLE bool type(const QString &text);
    /// The accessibility tree of the active session, as JSON text.
    Q_INVOKABLE QString accessibility();
    /// Create a session on the daemon.
    Q_INVOKABLE void createSession(const QString &name, const QString &command,
                                   bool agentOwned, const QString &owner);
    /// Stop a session on the daemon.
    Q_INVOKABLE void stopSession(const QString &name);

    /// A session's pending notes, newest last.
    Q_INVOKABLE QVariantList notes(const QString &session);
    /// Ask the daemon for a session's pending notes.
    Q_INVOKABLE void refreshNotes(const QString &session);
    /// Add a human note for a session, optionally with the annotated region as
    /// PNG bytes captured from the frame the human was looking at.
    Q_INVOKABLE void addNote(const QString &session, const QString &comment,
                             const QImage &region);
    /// Acknowledge one note.
    Q_INVOKABLE void resolveNote(const QString &session, int id);
    /// The PNG a note carries, base64-encoded; empty when it has none.
    Q_INVOKABLE QString noteImage(const QString &session, int id);

signals:
    void connectedChanged();
    void sessionsChanged();
    void activeNameChanged();
    void frameChanged();
    /// A session's pending notes changed.
    void notesChanged(const QString &session);

private:
    void send(const QByteArray &request);
    void handleResponse(const QJsonObject &response);
    void subscribe(const QString &session);
    void onStreamData();
    static QString controlSocketPath();
    static QString streamSocketPath();

    QLocalSocket *m_control = nullptr;
    QLocalSocket *m_stream = nullptr;
    bool m_connected = false;
    QVariantList m_sessions;
    QString m_activeName;
    QImage m_frame;
    QByteArray m_streamBuffer;
    /// Accumulates control responses until a newline completes one.
    QByteArray m_controlBuffer;
    /// Pending notes by session, so the panel can be driven from QML bindings.
    QHash<QString, QVariantList> m_notes;
    /// The session a feedback request was made for.
    QString m_notesSession;
};
