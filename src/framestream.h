// The frame stream: the daemon renders the active session and sends JPEG frames
// to whoever is watching.
//
// Rendering is driven by subscription, not by a timer: with no viewer attached
// the daemon still keeps the compositor's frame callbacks flowing (so clients
// stay interactive) but does not encode anything. That is what makes a windowless
// daemon cheap as well as correct.

#pragma once

#include <QByteArray>
#include <QHash>
#include <QObject>
#include <QString>

class LumenCompositor;
class QLocalServer;
class QLocalSocket;

class FrameStream : public QObject {
    Q_OBJECT

public:
    explicit FrameStream(LumenCompositor *compositor, QObject *parent = nullptr);
    ~FrameStream() override;

    /// Listen for viewers on `path`. Returns false when it is already taken.
    bool listen(const QString &path);
    QString path() const { return m_path; }

    /// Which session a viewer is watching.
    void subscribe(QLocalSocket *viewer, const QString &session);

private:
    void pump();

    LumenCompositor *m_compositor;
    QLocalServer *m_server = nullptr;
    QString m_path;
    /// The session each viewer is watching.
    QHash<QLocalSocket *, QString> m_subscriptions;
    /// The session each viewer was last sent a frame for, so a switch resets it.
    QHash<QLocalSocket *, QString> m_lastSession;
};
