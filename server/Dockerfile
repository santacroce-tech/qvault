FROM node:20-alpine

WORKDIR /app

COPY package.json package-lock.json* ./
RUN npm ci --production 2>/dev/null || npm install --production

COPY src/ ./src/

RUN mkdir -p /data

ENV PORT=3000
ENV DB_PATH=/data/licenses.db

EXPOSE 3000

CMD ["node", "src/index.js"]
