# 媒体文件 API

## 目录

- [上传媒体](#上传媒体)
- [下载媒体](#下载媒体)
- [缩略图](#缩略图)
- [媒体配置](#媒体配置)

---

## 上传媒体

### 上传媒体文件

**端点:** `POST /_matrix/media/v3/upload`

**需要认证:** 是

**请求格式:** `application/json`

**请求体:**
```typescript
interface UploadMediaRequest {
  content: string | number[];  // Base64 编码的字符串或字节数组
  filename?: string;           // 文件名
  content_type?: string;       // MIME 类型 (默认: application/octet-stream)
}
```

**请求示例 (Base64):**
```typescript
const uploadMediaBase64 = async (base64Content: string, filename: string, mimeType: string, accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/media/v3/upload`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      content: base64Content,
      filename,
      content_type: mimeType
    })
  });
  return handleApiResponse<{
    content_uri: string;
  }>(response);
};
```

**请求示例 (字节数组):**
```typescript
const uploadMediaBytes = async (bytes: number[], filename: string, mimeType: string, accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/media/v3/upload`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      content: bytes,
      filename,
      content_type: mimeType
    })
  });
  return handleApiResponse<{
    content_uri: string;
  }>(response);
};
```

**从 File 对象转换为 Base64 上传:**
```typescript
const fileToBase64 = (file: File): Promise<string> => {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result as string;
      // 移除 data:image/xxx;base64, 前缀
      const base64 = result.split(',')[1];
      resolve(base64);
    };
    reader.onerror = reject;
    reader.readAsDataURL(file);
  });
};

const uploadFile = async (file: File, accessToken: string) => {
  const base64 = await fileToBase64(file);
  return uploadMediaBase64(base64, file.name, file.type, accessToken);
};
```

**响应:**
```json
{
  "status": "ok",
  "data": {
    "content_uri": "mxc://cjystx.top/abcdef123456"
  }
}
```

---

## 下载媒体

### 下载媒体文件

**端点:** `GET /_matrix/media/v3/download/{server_name}/{media_id}`

**参数说明:**
- `server_name`: 服务器名 (如: `cjystx.top`)
- `media_id`: 媒体 ID (从 content_uri 中提取，如 `mxc://cjystx.top/abcdef123456` → `abcdef123456`)

**请求示例:**
```typescript
const downloadMedia = (contentUri: string) => {
  // 解析 content_uri: mxc://server/media_id
  const url = new URL(contentUri);
  const server = url.hostname;
  const mediaId = url.pathname.slice(1);

  return `${BASE_URL}/_matrix/media/v3/download/${server}/${mediaId}`;
};

// React 组件中使用
const ImageComponent: React.FC<{ uri: string; alt?: string }> = ({ uri, alt }) => {
  const src = downloadMedia(uri);

  return <img src={src} alt={alt} />;
};

// 或者直接下载文件
const downloadFile = async (contentUri: string, filename: string, accessToken: string) => {
  const url = downloadMedia(contentUri);

  const response = await fetch(url, {
    headers: { 'Authorization': `Bearer ${accessToken}` }
  });

  const blob = await response.blob();
  const blobUrl = window.URL.createObjectURL(blob);

  const a = document.createElement('a');
  a.href = blobUrl;
  a.download = filename;
  a.click();

  window.URL.revokeObjectURL(blobUrl);
};
```

---

## 缩略图

### 获取媒体缩略图

**端点:** `GET /_matrix/media/v3/thumbnail/{server_name}/{media_id}`

**查询参数:**
| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| width | number | 否 | 期望宽度 (默认 800) |
| height | number | 否 | 期望高度 (默认 600) |
| method | string | 否 | 缩放方法 (crop, scale，默认 scale) |

**请求示例:**
```typescript
const getThumbnailUrl = (
  contentUri: string,
  width = 256,
  height = 256,
  method = 'crop'
) => {
  const url = new URL(contentUri);
  const server = url.hostname;
  const mediaId = url.pathname.slice(1);

  const thumbnailUrl = `${BASE_URL}/_matrix/media/v3/thumbnail/${server}/${mediaId}?width=${width}&height=${height}&method=${method}`;
  return thumbnailUrl;
};

// React 组件中使用
const ThumbnailImage: React.FC<{ uri: string; width?: number; height?: number }> = ({
  uri,
  width = 256,
  height = 256
}) => {
  const src = getThumbnailUrl(uri, width, height);

  return <img src={src} alt="thumbnail" style={{ width, height }} />;
};
```

---

## 媒体配置

### 获取媒体配置

**端点:** `GET /_matrix/media/v1/config`

**响应:**
```json
{
  "m.upload.size": 52428800
}
```

**请求示例:**
```typescript
const getMediaConfig = async (accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/media/v1/config`, {
    headers: { 'Authorization': `Bearer ${accessToken}` }
  });
  return handleApiResponse<{
    'm.upload.size': number;   // 最大上传大小 (字节，默认 50MB)
  }>(response);
};
```

---

## 完整媒体服务示例

```typescript
class MediaService {
  constructor(private auth: AuthService) {}

  // 上传 Base64 数据
  async uploadBase64(
    base64Data: string,
    filename: string,
    mimeType: string
  ): Promise<string> {
    const response = await fetch(`${BASE_URL}/_matrix/media/v3/upload`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${this.auth.accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        content: base64Data,
        filename,
        content_type: mimeType
      })
    });

    const result = await this.auth.handleResponse<{ content_uri: string }>(response);
    return result.content_uri;
  }

  // 上传字节数组
  async uploadBytes(
    bytes: number[],
    filename: string,
    mimeType: string
  ): Promise<string> {
    const response = await fetch(`${BASE_URL}/_matrix/media/v3/upload`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${this.auth.accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        content: bytes,
        filename,
        content_type: mimeType
      })
    });

    const result = await this.auth.handleResponse<{ content_uri: string }>(response);
    return result.content_uri;
  }

  // 从 File 对象上传
  async uploadFile(file: File): Promise<string> {
    const base64 = await this.fileToBase64(file);
    return this.uploadBase64(base64, file.name, file.type);
  }

  private fileToBase64(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => {
        const result = reader.result as string;
        const base64 = result.split(',')[1];
        resolve(base64);
      };
      reader.onerror = reject;
      reader.readAsDataURL(file);
    });
  }

  // 获取下载 URL
  getDownloadUrl(contentUri: string): string {
    const url = new URL(contentUri);
    const server = url.hostname;
    const mediaId = url.pathname.slice(1);

    return `${BASE_URL}/_matrix/media/v3/download/${server}/${mediaId}`;
  }

  // 获取缩略图 URL
  getThumbnailUrl(
    contentUri: string,
    width = 256,
    height = 256,
    method: 'crop' | 'scale' = 'crop'
  ): string {
    const url = new URL(contentUri);
    const server = url.hostname;
    const mediaId = url.pathname.slice(1);

    return `${BASE_URL}/_matrix/media/v3/thumbnail/${server}/${mediaId}?width=${width}&height=${height}&method=${method}`;
  }

  // 下载文件
  async downloadFile(contentUri: string): Promise<Blob> {
    const url = this.getDownloadUrl(contentUri);
    const response = await fetch(url, {
      headers: { 'Authorization': `Bearer ${this.auth.accessToken}` }
    });
    return response.blob();
  }

  // 获取媒体配置
  async getConfig() {
    const response = await fetch(`${BASE_URL}/_matrix/media/v1/config`, {
      headers: { 'Authorization': `Bearer ${this.auth.accessToken}` }
    });
    return this.auth.handleResponse<{
      'm.upload.size': number;
    }>(response);
  }
}
```

---

## React Hook 示例

```typescript
import { useState, useCallback } from 'react';
import { MediaService } from './services/media';

interface UseMediaResult {
  uploading: boolean;
  progress: number;
  upload: (file: File) => Promise<string>;
  uploadBase64: (base64: string, filename: string, mimeType: string) => Promise<string>;
  getThumbnail: (uri: string, width?: number, height?: number) => string;
  download: (uri: string) => Promise<void>;
  getDownloadUrl: (uri: string) => string;
}

export function useMedia(accessToken: string): UseMediaResult {
  const [uploading, setUploading] = useState(false);
  const [progress, setProgress] = useState(0);

  const mediaService = new MediaService(new AuthService(accessToken));

  const upload = useCallback(async (file: File) => {
    setUploading(true);
    setProgress(0);

    try {
      const contentUri = await mediaService.uploadFile(file);
      setProgress(100);
      return contentUri;
    } finally {
      setUploading(false);
      setProgress(0);
    }
  }, [accessToken]);

  const uploadBase64 = useCallback(async (
    base64: string,
    filename: string,
    mimeType: string
  ) => {
    setUploading(true);
    try {
      return await mediaService.uploadBase64(base64, filename, mimeType);
    } finally {
      setUploading(false);
    }
  }, [accessToken]);

  const getThumbnail = useCallback((
    uri: string,
    width = 256,
    height = 256
  ) => {
    return mediaService.getThumbnailUrl(uri, width, height);
  }, [accessToken]);

  const download = useCallback(async (uri: string) => {
    const blob = await mediaService.downloadFile(uri);
    const url = window.URL.createObjectURL(blob);

    const a = document.createElement('a');
    a.href = url;
    a.download = `file_${Date.now()}`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);

    window.URL.revokeObjectURL(url);
  }, [accessToken]);

  const getDownloadUrl = useCallback((uri: string) => {
    return mediaService.getDownloadUrl(uri);
  }, [accessToken]);

  return {
    uploading,
    progress,
    upload,
    uploadBase64,
    getThumbnail,
    download,
    getDownloadUrl
  };
}
```

---

## 媒体类型常量

```typescript
// 支持的 MIME 类型
export const SUPPORTED_IMAGE_TYPES = [
  'image/jpeg',
  'image/png',
  'image/gif',
  'image/webp',
  'image/svg+xml'
];

export const SUPPORTED_AUDIO_TYPES = [
  'audio/ogg',
  'audio/mpeg',
  'audio/wav',
  'audio/webm'
];

export const SUPPORTED_VIDEO_TYPES = [
  'video/mp4',
  'video/webm',
  'video/ogg'
];

export const MAX_FILE_SIZE = 50 * 1024 * 1024; // 50MB
export const MAX_IMAGE_SIZE = 10 * 1024 * 1024; // 10MB

// 文件类型验证
export function validateImageFile(file: File): boolean {
  return SUPPORTED_IMAGE_TYPES.includes(file.type) &&
         file.size <= MAX_IMAGE_SIZE;
}

export function validateAudioFile(file: File): boolean {
  return SUPPORTED_AUDIO_TYPES.includes(file.type) &&
         file.size <= MAX_FILE_SIZE;
}

export function validateVideoFile(file: File): boolean {
  return SUPPORTED_VIDEO_TYPES.includes(file.type) &&
         file.size <= MAX_FILE_SIZE;
}
```

---

## 注意事项

1. **上传格式**: 此服务器使用 JSON 格式上传媒体，支持 Base64 编码字符串或字节数组
2. **文件大小限制**: 默认最大上传大小为 50MB
3. **认证**: 所有媒体上传操作需要认证
4. **内容 URI**: 上传成功后返回的 `content_uri` 格式为 `mxc://server_name/media_id`
5. **缩略图**: 缩略图默认尺寸为 800x600，可通过查询参数自定义
