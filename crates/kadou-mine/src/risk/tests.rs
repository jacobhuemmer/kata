//! Risk-defaulting tests (`docs/design/06-session-mining.md` §4.4).

use super::*;

#[test]
fn mutating_verbs_default_to_high() {
    for template in [
        "kubectl --context $CONTEXT apply -f $PATH_1",
        "kubectl --context $CONTEXT -n $NAMESPACE delete pod $VAL",
        "helm upgrade release chart",
        "helm uninstall release",
        "terraform apply",
        "docker push $HOST/$VAL",
    ] {
        assert_eq!(default_risk(template), RiskLevel::High, "{template}");
    }
}

#[test]
fn git_mutating_verbs_default_to_high_via_the_catchall() {
    // `git` only has an explicit rule for the read-only verbs below (log/status/diff ->
    // Medium); a mutating verb like `push --force` matches no arm at all and falls through
    // to `default_risk`'s `unwrap_or(RiskLevel::High)`, the same path an unrecognized tool
    // takes. Pins that fall-through outcome, not a dedicated `git` rule -- there isn't one.
    for template in [
        "git push --force",
        "git push --force-with-lease origin main",
        "git reset --hard HEAD~1",
        "git commit -am wip",
    ] {
        assert_eq!(default_risk(template), RiskLevel::High, "{template}");
    }
}

#[test]
fn read_only_verbs_default_to_medium_never_low() {
    for template in [
        "kubectl --context $CONTEXT -n $NAMESPACE get pods -l app=$APP",
        "kubectl -n $NAMESPACE describe pod $VAL",
        "kubectl -n $NAMESPACE logs deploy/$APP",
        "helm template chart",
        "helm status release",
        "git log --oneline",
        "git status",
        "git diff HEAD",
    ] {
        let risk = default_risk(template);
        assert_eq!(risk, RiskLevel::Medium, "{template}");
        assert_ne!(risk, RiskLevel::Low);
    }
}

#[test]
fn unrecognized_or_mixed_templates_default_to_high_never_critical() {
    let risk = default_risk("some-custom-tool --do-a-thing $VAL");
    assert_eq!(risk, RiskLevel::High);
    assert_ne!(risk, RiskLevel::Critical);
}

#[test]
fn never_defaults_to_low_or_critical_across_every_signal() {
    for template in [
        "kubectl apply -f $PATH_1",
        "kubectl get pods",
        "totally unknown command",
    ] {
        let risk = default_risk(template);
        assert!(
            risk == RiskLevel::Medium || risk == RiskLevel::High,
            "{template}: {risk}"
        );
    }
}
