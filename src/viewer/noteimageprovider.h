// Serves a note's annotated crop to QML.
//
// The viewer's feedback list renders each note's screenshot through
// `image://lumen-note/<session>/<id>`. That URL had no provider registered, so
// every note image silently resolved to nothing — the note carried its pixels
// in the daemon's store and the list could never show them.
//
// The crop is fetched per request rather than kept in memory: notes persist and
// may be many, and the viewer has no reason to hold every one of them.

#pragma once

#include <QImage>
#include <QQuickImageProvider>

class DaemonClient;

class NoteImageProvider : public QQuickImageProvider {
  public:
    explicit NoteImageProvider(DaemonClient* client);

    /// `id` is `<session>/<noteId>`, as produced by the feedback list's URL.
    QImage requestImage(const QString& id, QSize* size, const QSize& requestedSize) override;

  private:
    DaemonClient* m_client;
};
