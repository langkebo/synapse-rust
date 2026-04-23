// Voice storage — now a thin adapter. Audio files are stored via MediaService.
// Voice metadata (duration, waveform) is carried in Matrix event content
// using m.audio msgtype + org.matrix.msc3245.voice extension.
//
// The voice_messages and voice_usage_stats tables are no longer used.
// They remain in the schema for backward compatibility during migration.
