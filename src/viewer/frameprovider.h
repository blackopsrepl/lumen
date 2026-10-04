// Serves the daemon's latest frame to QML.
//
// An `Image` source is a URL, and a QImage cannot be assigned to it, so the
// frame is exposed through the image provider protocol instead. The viewer asks
// for `image://lumen/frame/<token>`, where the token changes with every frame:
// that is what makes QML re-request the image, since an unchanged URL would be
// served from cache and the view would freeze on the first frame.

#pragma once

#include <QImage>
#include <QQuickImageProvider>

class DaemonClient;

class FrameProvider : public QQuickImageProvider {
public:
    explicit FrameProvider(DaemonClient *client);

    /// The frame named by `id`, which is the client's current frame.
    QImage requestImage(const QString &id, QSize *size, const QSize &requestedSize) override;

private:
    DaemonClient *m_client;
};
