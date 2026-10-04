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
    /// URL for the current frame, changing with every new frame.
    ///
    /// An Image source is a URL and a QImage cannot be assigned to it, so the
    /// frame is served by the image provider. The token must change or QML will
    /// serve the first frame from cache and the view will freeze.
    Q_PROPERTY(QString frameUrl READ frameUrl NOTIFY frameChanged)

  public:
    explicit DaemonClient(QObject* parent = nullptr);

    bool connected() const { return m_connected; }
    QVariantList sessions() const { return m_sessions; }
    QString activeName() const { return m_activeName; }
    /// Select the session to watch. Invokable so QML can call it directly, not
    /// only through the property.
    Q_INVOKABLE void setActiveName(const QString& name);
    QImage frame() const { return m_frame; }
    QString frameUrl() const { return m_frameUrl; }

    /// Reconnect and refresh the session list.
    Q_INVOKABLE void refresh();
    /// Deliver a click at a point in the active surface's coordinates.
    Q_INVOKABLE bool click(qreal x, qreal y);
    /// Deliver text to the active session.
    Q_INVOKABLE bool type(const QString& text);
    /// The accessibility tree of the active session, as JSON text.
    Q_INVOKABLE QString accessibility();
    /// Stop a session on the daemon. Sessions are started by an agent, never by
    /// the viewer, so there is no create counterpart here.
    Q_INVOKABLE void stopSession(const QString& name);

    /// A session's pending notes, newest last.
    Q_INVOKABLE QVariantList notes(const QString& session);
    /// Ask the daemon for a session's pending notes.
    Q_INVOKABLE void refreshNotes(const QString& session);
    /// Record a human note on a session, cropping the annotated region out of
    /// the frame currently on screen.
    ///
    /// The region arrives as plain geometry in *frame* pixels — the crop is done
    /// here, in C++, because a QImage cannot be built from QML: `QImage::copy`
    /// is not invokable, so the crop that used to live in the viewer's
    /// `captureRegion()` threw at the call site and the note was never sent, and
    /// QML gave no error for it. Passing the rectangle is also the easier
    /// contract — the caller is already working in frame coordinates.
    Q_INVOKABLE void addNote(const QString& session, const QString& comment, int x, int y, int w,
                             int h);
    /// Acknowledge one note.
    Q_INVOKABLE void resolveNote(const QString& session, int id);
    /// The PNG a note carries, base64-encoded; empty when it has none.
    Q_INVOKABLE QString noteImage(const QString& session, int id);

  signals:
    void connectedChanged();
    void sessionsChanged();
    void activeNameChanged();
    void frameChanged();
    /// A session's pending notes changed.
    void notesChanged(const QString& session);

  private:
    void send(const QByteArray& request);
    void handleResponse(const QJsonObject& response);
    void subscribe(const QString& session);
    void onStreamData();
    static QString controlSocketPath();
    static QString streamSocketPath();

    QLocalSocket* m_control = nullptr;
    QLocalSocket* m_stream = nullptr;
    bool m_connected = false;
    QVariantList m_sessions;
    QString m_activeName;
    QImage m_frame;
    QString m_frameUrl;
    /// Increments per frame so each one gets a distinct URL.
    quint64 m_frameSerial = 0;
    QByteArray m_streamBuffer;
    /// Accumulates control responses until a newline completes one.
    QByteArray m_controlBuffer;
    /// Pending notes by session, so the panel can be driven from QML bindings.
    QHash<QString, QVariantList> m_notes;
    /// The session a feedback request was made for.
    QString m_notesSession;
};
