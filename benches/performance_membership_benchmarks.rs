use criterion::{black_box, criterion_group, criterion_main, Criterion};
use synapse_common::membership_transition::{is_legal, JoinRule, TransitionCtx};
use synapse_common::Membership;

fn state_only_ctx(actor_is_target: bool, target_is_banned: bool) -> TransitionCtx {
    TransitionCtx::state_only(JoinRule::Public, actor_is_target, target_is_banned, false)
}

fn knock_ctx(actor_is_target: bool) -> TransitionCtx {
    TransitionCtx::state_only(JoinRule::Knock, actor_is_target, false, false)
}

fn bench_membership_transitions(c: &mut Criterion) {
    let mut group = c.benchmark_group("membership_transitions");
    group.sample_size(100);
    group.measurement_time(std::time::Duration::from_secs(5));

    let cases: &[(&str, Option<Membership>, Membership, TransitionCtx, bool)] = &[
        // Fail-closed paths (must reject)
        ("ban_to_join", Some(Membership::Ban), Membership::Join, state_only_ctx(true, false), false),
        ("ban_to_invite", Some(Membership::Ban), Membership::Invite, state_only_ctx(false, false), false),
        ("ban_to_knock", Some(Membership::Ban), Membership::Knock, knock_ctx(true), false),
        ("self_ban", Some(Membership::Join), Membership::Ban, state_only_ctx(true, false), false),
        ("banned_self_leave", Some(Membership::Ban), Membership::Leave, state_only_ctx(true, false), false),
        ("invite_of_banned", Some(Membership::Ban), Membership::Invite, state_only_ctx(false, true), false),
        ("join_restricted_not_invited", None, Membership::Join,
         TransitionCtx::state_only(JoinRule::Invite, true, false, false), false),
        // Allowed paths
        ("invite_to_join", Some(Membership::Invite), Membership::Join, state_only_ctx(true, false), true),
        ("leave_to_invite", Some(Membership::Leave), Membership::Invite, state_only_ctx(false, false), true),
        ("knock_to_join", Some(Membership::Knock), Membership::Join, state_only_ctx(true, false), true),
        // Idempotent
        ("leave_to_leave", Some(Membership::Leave), Membership::Leave, state_only_ctx(true, false), true),
        ("join_to_join", Some(Membership::Join), Membership::Join, state_only_ctx(true, false), true),
        ("knock_to_knock", Some(Membership::Knock), Membership::Knock, knock_ctx(true), true),
        // New user join (no prior membership)
        ("none_to_join", None, Membership::Join, state_only_ctx(true, false), true),
    ];

    for (name, from, to, ctx, should_pass) in cases {
        group.bench_function(*name, |b| {
            b.iter(|| {
                let result = is_legal(black_box(*from), black_box(*to), black_box(ctx));
                if *should_pass {
                    assert!(result.is_ok(), "expected legal: {:?} -> {:?}", from, to);
                } else {
                    assert!(result.is_err(), "expected illegal: {:?} -> {:?}", from, to);
                }
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_membership_transitions);
criterion_main!(benches);
