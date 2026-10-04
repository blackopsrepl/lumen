// Durable per-session inbox for human feedback.
//
// A note may carry a screenshot of the exact region the human drew on, captured
// when the note is sent, so it still shows what they meant after the surface
// has changed. The schema and the consume/ack semantics match the ones agents
// already expect: `consume` returns exactly the notes it acknowledges.

#pragma once

#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QSqlDatabase>
#include <QString>

class FeedbackStore : public QObject {
    Q_OBJECT

public:
    explicit FeedbackStore(const QString &path, QObject *parent = nullptr);
    ~FeedbackStore() override;

    /// Store a note, optionally with a PNG of the annotated region.
    Q_INVOKABLE QJsonObject add(const QString &session, const QString &author,
                                const QString &comment, const QByteArray &screenshot);

    /// A session's notes, newest last. `pendingOnly` limits to unread ones.
    Q_INVOKABLE QJsonArray list(const QString &session, bool pendingOnly) const;

    /// Return a session's pending notes and acknowledge them in one
    /// transaction. A note arriving between a separate read and a separate
    /// ack would otherwise be acknowledged without ever being shown.
    Q_INVOKABLE QJsonArray consume(const QString &session);

    /// Acknowledge one note. Returns whether the note exists.
    Q_INVOKABLE bool ack(const QString &session, qint64 id);

    /// The PNG attached to a note, or a null array when it has none.
    QByteArray screenshot(const QString &session, qint64 id) const;

private:
    void migrate();

    QSqlDatabase m_db;
};
