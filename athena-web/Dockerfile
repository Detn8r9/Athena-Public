# Athena Web core — lean UI + API orchestration image
# Heavy models/engines are downloaded via the module manager, not baked in.

FROM node:22-bookworm-slim AS frontend-build
WORKDIR /src/frontend
COPY frontend/package.json frontend/package-lock.json* ./
RUN npm install --no-audit --no-fund
COPY frontend/ ./
RUN npm run build

FROM python:3.12-slim-bookworm AS runtime
ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    ATHENA_HOST=0.0.0.0 \
    ATHENA_PORT=8080 \
    ATHENA_DATA_DIR=/data \
    ATHENA_CATALOG_DIR=/app/catalogs \
    ATHENA_STATIC_DIR=/app/static

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
      curl ca-certificates ffmpeg \
    && rm -rf /var/lib/apt/lists/*

COPY backend/requirements.txt /app/backend/requirements.txt
RUN pip install --no-cache-dir -r /app/backend/requirements.txt

COPY backend /app/backend
COPY catalogs /app/catalogs
COPY scripts /app/scripts
COPY --from=frontend-build /src/frontend/dist /app/static

ENV PYTHONPATH=/app

RUN mkdir -p /data/models /data/modules /data/apps /data/layout \
    && chmod +x /app/scripts/*.sh

EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8080/api/health || exit 1

CMD ["bash", "/app/scripts/entrypoint.sh"]
