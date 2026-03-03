{{/*
Expand the name of the chart.
*/}}
{{- define "gate.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "gate.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "gate.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "gate.labels" -}}
helm.sh/chart: {{ include "gate.chart" . }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
{{- end }}

{{/*
Proxy selector labels
*/}}
{{- define "gate.proxy.selectorLabels" -}}
app.kubernetes.io/name: {{ include "gate.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/component: proxy
{{- end }}

{{/*
Admin selector labels
*/}}
{{- define "gate.admin.selectorLabels" -}}
app.kubernetes.io/name: {{ include "gate.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/component: admin
{{- end }}

{{/*
Service account name
*/}}
{{- define "gate.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "gate.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Image tag — defaults to Chart.appVersion
*/}}
{{- define "gate.imageTag" -}}
{{- default .Chart.AppVersion .Values.image.tag }}
{{- end }}

{{/*
Construct DATABASE_URL from postgresql subchart values, or use explicit config value.
*/}}
{{- define "gate.databaseUrl" -}}
{{- if .Values.config.databaseUrl }}
{{- .Values.config.databaseUrl }}
{{- else if .Values.postgresql.enabled }}
{{- printf "postgres://%s:%s@%s-postgresql:5432/%s" .Values.postgresql.auth.username .Values.postgresql.auth.password (include "gate.fullname" .) .Values.postgresql.auth.database }}
{{- else }}
{{- required "config.databaseUrl is required when postgresql.enabled is false" .Values.config.databaseUrl }}
{{- end }}
{{- end }}

{{/*
Construct REDIS_URL from redis subchart values, or use explicit config value.
*/}}
{{- define "gate.redisUrl" -}}
{{- if .Values.config.redisUrl }}
{{- .Values.config.redisUrl }}
{{- else if .Values.redis.enabled }}
{{- printf "redis://%s-redis-master:6379" (include "gate.fullname" .) }}
{{- end }}
{{- end }}

{{/*
Secret name — use existing or generated
*/}}
{{- define "gate.secretName" -}}
{{- if .Values.secrets.existingSecret }}
{{- .Values.secrets.existingSecret }}
{{- else }}
{{- include "gate.fullname" . }}
{{- end }}
{{- end }}
