#!/bin/sh
set -eu

K8S_CONTEXT="${K8S_CONTEXT:?k8s_context is required}"
ARGOCD_NAMESPACE="${ARGOCD_NAMESPACE:-argocd}"
APP_NAME="${APP_NAME:?app_name is required}"
TIMEOUT="${TIMEOUT:-60}"

get_sync_status() {
  kubectl --context "${K8S_CONTEXT}" -n "${ARGOCD_NAMESPACE}" \
    get application "${APP_NAME}" \
    -o jsonpath='{.status.sync.status}' 2>/dev/null
}

get_values_revision() {
  kubectl --context "${K8S_CONTEXT}" -n "${ARGOCD_NAMESPACE}" \
    get application "${APP_NAME}" \
    -o jsonpath='{.status.sync.revisions[1]}' 2>/dev/null
}

get_chart_revision() {
  kubectl --context "${K8S_CONTEXT}" -n "${ARGOCD_NAMESPACE}" \
    get application "${APP_NAME}" \
    -o jsonpath='{.status.sync.revisions[0]}' 2>/dev/null
}

get_health_status() {
  kubectl --context "${K8S_CONTEXT}" -n "${ARGOCD_NAMESPACE}" \
    get application "${APP_NAME}" \
    -o jsonpath='{.status.health.status}' 2>/dev/null
}

get_last_sync_time() {
  kubectl --context "${K8S_CONTEXT}" -n "${ARGOCD_NAMESPACE}" \
    get application "${APP_NAME}" \
    -o jsonpath='{.status.operationState.finishedAt}' 2>/dev/null
}

main() {
  echo "==> Stage 1/4: Validate"
  echo "    Context   : ${K8S_CONTEXT}"
  echo "    Namespace : ${ARGOCD_NAMESPACE}"
  echo "    App       : ${APP_NAME}"

  if ! kubectl --context "${K8S_CONTEXT}" -n "${ARGOCD_NAMESPACE}" \
    get application "${APP_NAME}" > /dev/null 2>&1; then
    echo "✗ Application '${APP_NAME}' not found in ${ARGOCD_NAMESPACE}" >&2
    exit 1
  fi

  echo ""
  echo "==> Stage 2/4: Capture pre-refresh state"
  PRE_STATUS="$(get_sync_status)"
  PRE_REVISION="$(get_values_revision)"
  PRE_CHART="$(get_chart_revision)"
  echo "    Sync status : ${PRE_STATUS}"
  echo "    Chart ver   : ${PRE_CHART}"
  echo "    Values rev  : ${PRE_REVISION}"

  echo ""
  echo "==> Stage 3/4: Trigger refresh"
  kubectl --context "${K8S_CONTEXT}" -n "${ARGOCD_NAMESPACE}" \
    patch application "${APP_NAME}" \
    --type merge \
    -p '{"metadata":{"annotations":{"argocd.argoproj.io/refresh":"normal"}}}' \
    > /dev/null
  echo "    Refresh annotation applied"

  echo "    Waiting for ArgoCD to reconcile..."
  ELAPSED=0
  INTERVAL=5
  while [ "${ELAPSED}" -lt "${TIMEOUT}" ]; do
    sleep "${INTERVAL}"
    ELAPSED=$((ELAPSED + INTERVAL))
    CURRENT_REV="$(get_values_revision)"
    CURRENT_STATUS="$(get_sync_status)"
    if [ "${CURRENT_REV}" != "${PRE_REVISION}" ] || [ "${CURRENT_STATUS}" = "Synced" ]; then
      break
    fi
    printf "    ...%ds elapsed (status: %s)\n" "${ELAPSED}" "${CURRENT_STATUS}"
  done

  echo ""
  echo "==> Stage 4/4: Verify"
  POST_STATUS="$(get_sync_status)"
  POST_REVISION="$(get_values_revision)"
  POST_CHART="$(get_chart_revision)"
  POST_HEALTH="$(get_health_status)"
  POST_SYNC_TIME="$(get_last_sync_time)"

  REVISION_CHANGED="no"
  if [ "${POST_REVISION}" != "${PRE_REVISION}" ]; then
    REVISION_CHANGED="yes"
  fi

  echo "========================================="
  echo "  ArgoCD Sync Result"
  echo "========================================="
  echo "  App             : ${APP_NAME}"
  echo "  Context         : ${K8S_CONTEXT}"
  echo "  Sync status     : ${POST_STATUS}"
  echo "  Health          : ${POST_HEALTH}"
  echo "  Chart version   : ${POST_CHART}"
  echo "  Values revision : ${POST_REVISION}"
  echo "  Revision changed: ${REVISION_CHANGED}"
  echo "  Last sync       : ${POST_SYNC_TIME}"
  echo "========================================="

  if [ "${POST_STATUS}" = "OutOfSync" ]; then
    echo ""
    echo "! Application is OutOfSync — ArgoCD detected drift."
    echo "  This is expected if auto-sync is disabled."
    echo "  A manual sync may be needed via ArgoCD UI or CLI."
  fi

  if [ "${POST_STATUS}" = "Synced" ]; then
    echo ""
    echo "✓ Done — application is synced"
  fi
}

main "$@"
