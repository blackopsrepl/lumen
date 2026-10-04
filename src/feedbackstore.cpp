#include "feedbackstore.h"

#include <QDateTime>
#include <QDebug>
#include <QDir>
#include <QFileInfo>
#include <QSqlError>
#include <QSqlQuery>
#include <QVariant>

namespace {

QJsonObject rowToObject(const QSqlQuery& query) {
    QJsonObject note;
    note[QStringLiteral("id")] = query.value(0).toLongLong();
    note[QStringLiteral("session")] = query.value(1).toString();
    note[QStringLiteral("created_at")] = query.value(2).toLongLong();
    note[QStringLiteral("author")] = query.value(3).toString();
    note[QStringLiteral("comment")] = query.value(4).toString();
    note[QStringLiteral("screenshot")] = query.value(5).toBool();
    note[QStringLiteral("status")] = query.value(6).toString();
    return note;
}

constexpr char kSelect[] = "SELECT id, session, created_at, author, comment, "
                           "(screenshot IS NOT NULL) AS has_screenshot, status FROM feedback ";

} // namespace

FeedbackStore::FeedbackStore(const QString& path, QObject* parent) : QObject(parent) {
    const QFileInfo info(path);
    QDir().mkpath(info.absolutePath());
    m_db = QSqlDatabase::addDatabase(QStringLiteral("QSQLITE"), QStringLiteral("lumen-feedback"));
    m_db.setDatabaseName(path);
    if (!m_db.open()) {
        // Remember why. Every query below then fails, and without this the only
        // symptom is notes that save and never appear.
        m_openError = m_db.lastError().text();
    }
    QSqlQuery query(m_db);
    query.exec(QStringLiteral("CREATE TABLE IF NOT EXISTS feedback ("
                              " id INTEGER PRIMARY KEY AUTOINCREMENT,"
                              " session TEXT NOT NULL,"
                              " created_at INTEGER NOT NULL,"
                              " author TEXT NOT NULL,"
                              " comment TEXT NOT NULL,"
                              " screenshot BLOB,"
                              " status TEXT NOT NULL DEFAULT 'pending')"));
    query.exec(QStringLiteral(
        "CREATE INDEX IF NOT EXISTS feedback_session_status ON feedback (session, status)"));
    migrate();
}

FeedbackStore::~FeedbackStore() {
    m_db.close();
    m_db = QSqlDatabase();
    QSqlDatabase::removeDatabase(QStringLiteral("lumen-feedback"));
}

void FeedbackStore::migrate() {
    // Add the screenshot column to a database created before notes carried
    // images. Older rows read back with no screenshot, which is the best that
    // can be recovered from stale pixels.
    QSqlQuery columns(m_db);
    columns.exec(QStringLiteral("PRAGMA table_info(feedback)"));
    bool hasScreenshot = false;
    while (columns.next()) {
        if (columns.value(1).toString() == QLatin1String("screenshot")) {
            hasScreenshot = true;
        }
    }
    if (!hasScreenshot) {
        QSqlQuery alter(m_db);
        alter.exec(QStringLiteral("ALTER TABLE feedback ADD COLUMN screenshot BLOB"));
    }
}

QJsonObject FeedbackStore::add(const QString& session, const QString& author,
                               const QString& comment, const QByteArray& screenshot) {
    const qint64 createdAt = QDateTime::currentSecsSinceEpoch();
    QSqlQuery query(m_db);
    query.prepare(QStringLiteral(
        "INSERT INTO feedback (session, created_at, author, comment, screenshot, status)"
        " VALUES (?, ?, ?, ?, ?, 'pending')"));
    query.addBindValue(session);
    query.addBindValue(createdAt);
    query.addBindValue(author);
    query.addBindValue(comment);
    query.addBindValue(screenshot.isEmpty() ? QVariant() : QVariant(screenshot));
    if (!query.exec()) {
        // A note that does not save is worse than an error: the human believes
        // it was left. The insert used to go unchecked, so a failure produced a
        // note object carrying `id: 0` and the note simply never appeared.
        qWarning("lumen: could not save note for %s: %s", qPrintable(session),
                 qPrintable(query.lastError().text()));
        m_lastError = query.lastError().text();
        return QJsonObject();
    }

    QJsonObject note;
    note[QStringLiteral("id")] = query.lastInsertId().toLongLong();
    note[QStringLiteral("session")] = session;
    note[QStringLiteral("created_at")] = createdAt;
    note[QStringLiteral("author")] = author;
    note[QStringLiteral("comment")] = comment;
    note[QStringLiteral("screenshot")] = !screenshot.isEmpty();
    note[QStringLiteral("status")] = QStringLiteral("pending");
    return note;
}

QJsonArray FeedbackStore::list(const QString& session, bool pendingOnly) const {
    QSqlQuery query(m_db);
    QString sql = QString::fromLatin1(kSelect);
    if (pendingOnly) {
        sql += QStringLiteral("WHERE session = ? AND status = 'pending' ORDER BY id");
    } else {
        sql += QStringLiteral("WHERE session = ? ORDER BY id");
    }
    query.prepare(sql);
    query.addBindValue(session);
    query.exec();
    QJsonArray notes;
    while (query.next()) {
        notes.append(rowToObject(query));
    }
    return notes;
}

QJsonArray FeedbackStore::consume(const QString& session) {
    m_db.transaction();
    QJsonArray notes = list(session, true);
    if (!notes.isEmpty()) {
        QSqlQuery update(m_db);
        update.prepare(QStringLiteral(
            "UPDATE feedback SET status = 'acked' WHERE session = ? AND status = 'pending'"));
        update.addBindValue(session);
        update.exec();
    }
    m_db.commit();
    return notes;
}

bool FeedbackStore::ack(const QString& session, qint64 id) {
    QSqlQuery query(m_db);
    query.prepare(
        QStringLiteral("UPDATE feedback SET status = 'acked' WHERE session = ? AND id = ?"));
    query.addBindValue(session);
    query.addBindValue(id);
    query.exec();
    return query.numRowsAffected() > 0;
}

QByteArray FeedbackStore::screenshot(const QString& session, qint64 id) const {
    QSqlQuery query(m_db);
    query.prepare(QStringLiteral("SELECT screenshot FROM feedback WHERE session = ? AND id = ?"));
    query.addBindValue(session);
    query.addBindValue(id);
    query.exec();
    if (query.next()) {
        return query.value(0).toByteArray();
    }
    return QByteArray();
}
