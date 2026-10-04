#include "frameprovider.h"

#include "daemonclient.h"

FrameProvider::FrameProvider(DaemonClient *client)
    : QQuickImageProvider(QQuickImageProvider::Image), m_client(client) {}

QImage FrameProvider::requestImage(const QString &id, QSize *size, const QSize &requestedSize) {
    Q_UNUSED(id);
    const QImage frame = m_client->frame();
    if (size) {
        *size = frame.size();
    }
    if (frame.isNull()) {
        return frame;
    }
    if (requestedSize.isValid() && requestedSize != frame.size()) {
        return frame.scaled(requestedSize, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    }
    return frame;
}
