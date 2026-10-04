#include "noteimageprovider.h"

#include "daemonclient.h"

#include <QByteArray>
#include <QDebug>
#include <QStringList>

NoteImageProvider::NoteImageProvider(DaemonClient* client)
    : QQuickImageProvider(QQuickImageProvider::Image), m_client(client) {}

QImage NoteImageProvider::requestImage(const QString& id, QSize* size, const QSize& requestedSize) {
    const QStringList parts = id.split(QLatin1Char('/'));
    if (parts.size() != 2) {
        return QImage();
    }
    bool ok = false;
    const int noteId = parts.at(1).toInt(&ok);
    if (!ok) {
        qWarning("lumen: note image asked for a non-numeric id: %s", qPrintable(id));
        return QImage();
    }

    // The daemon returns the stored PNG base64-encoded.
    const QByteArray png =
        QByteArray::fromBase64(m_client->noteImage(parts.at(0), noteId).toLatin1());
    QImage image;
    if (!image.loadFromData(png, "PNG")) {
        qWarning("lumen: note image %s: %d base64 bytes, but they are not a PNG", qPrintable(id),
                 png.size());
        return QImage();
    }
    if (size) {
        *size = image.size();
    }
    if (requestedSize.isValid() && requestedSize != image.size()) {
        return image.scaled(requestedSize, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    }
    return image;
}
