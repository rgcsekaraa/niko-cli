package llm

import (
	"archive/zip"
	"compress/gzip"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"time"

	"github.com/niko-cli/niko/internal/config"
)

const (
	OllamaVersion = "0.5.4"
)

type OllamaManager struct {
	binPath    string
	dataDir    string
	modelsDir  string
	isInternal bool
}

func NewOllamaManager() *OllamaManager {
	nikoDir := config.GetConfigDir()
	return &OllamaManager{
		binPath:   filepath.Join(nikoDir, "bin", getOllamaBinaryName()),
		dataDir:   filepath.Join(nikoDir, "ollama"),
		modelsDir: filepath.Join(nikoDir, "ollama", "models"),
	}
}

func getOllamaBinaryName() string {
	if runtime.GOOS == "windows" {
		return "ollama.exe"
	}
	return "ollama"
}

func (m *OllamaManager) GetBinPath() string {
	return m.binPath
}

func (m *OllamaManager) IsInstalled() bool {
	_, err := os.Stat(m.binPath)
	return err == nil
}

func (m *OllamaManager) IsSystemOllamaAvailable() bool {
	_, err := exec.LookPath("ollama")
	return err == nil
}

func (m *OllamaManager) EnsureInstalled(progressFn func(status string, pct float64)) error {
	if m.IsInstalled() {
		m.isInternal = true
		return nil
	}

	if m.IsSystemOllamaAvailable() {
		m.isInternal = false
		return nil
	}

	return m.downloadOllama(progressFn)
}

func (m *OllamaManager) downloadOllama(progressFn func(status string, pct float64)) error {
	url := m.getDownloadURL()
	if url == "" {
		return fmt.Errorf("unsupported platform: %s/%s", runtime.GOOS, runtime.GOARCH)
	}

	if progressFn != nil {
		progressFn("Downloading Ollama...", 0)
	}

	binDir := filepath.Dir(m.binPath)
	if err := os.MkdirAll(binDir, 0755); err != nil {
		return fmt.Errorf("failed to create bin directory: %w", err)
	}

	resp, err := http.Get(url)
	if err != nil {
		return fmt.Errorf("failed to download Ollama: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("failed to download Ollama: HTTP %d", resp.StatusCode)
	}

	tmpFile, err := os.CreateTemp("", "ollama-download-*")
	if err != nil {
		return err
	}
	defer os.Remove(tmpFile.Name())

	total := resp.ContentLength
	var downloaded int64
	buf := make([]byte, 32*1024)

	for {
		n, err := resp.Body.Read(buf)
		if n > 0 {
			tmpFile.Write(buf[:n])
			downloaded += int64(n)
			if progressFn != nil && total > 0 {
				pct := float64(downloaded) / float64(total) * 100
				progressFn("Downloading Ollama...", pct)
			}
		}
		if err == io.EOF {
			break
		}
		if err != nil {
			return err
		}
	}
	tmpFile.Close()

	if progressFn != nil {
		progressFn("Extracting Ollama...", 100)
	}

	if err := m.extractOllama(tmpFile.Name()); err != nil {
		return fmt.Errorf("failed to extract Ollama: %w", err)
	}

	if err := os.Chmod(m.binPath, 0755); err != nil {
		return fmt.Errorf("failed to set permissions: %w", err)
	}

	m.isInternal = true
	return nil
}

func (m *OllamaManager) getDownloadURL() string {
	base := fmt.Sprintf("https://github.com/ollama/ollama/releases/download/v%s", OllamaVersion)

	switch runtime.GOOS {
	case "darwin":
		return fmt.Sprintf("%s/ollama-darwin", base)
	case "linux":
		switch runtime.GOARCH {
		case "amd64":
			return fmt.Sprintf("%s/ollama-linux-amd64.tgz", base)
		case "arm64":
			return fmt.Sprintf("%s/ollama-linux-arm64.tgz", base)
		}
	case "windows":
		return fmt.Sprintf("%s/ollama-windows-amd64.zip", base)
	}
	return ""
}

func (m *OllamaManager) extractOllama(archivePath string) error {
	switch runtime.GOOS {
	case "darwin":
		return m.copyBinary(archivePath)
	case "linux":
		return m.extractTarGz(archivePath)
	case "windows":
		return m.extractZip(archivePath)
	}
	return fmt.Errorf("unsupported platform")
}

func (m *OllamaManager) copyBinary(src string) error {
	srcFile, err := os.Open(src)
	if err != nil {
		return err
	}
	defer srcFile.Close()

	dstFile, err := os.Create(m.binPath)
	if err != nil {
		return err
	}
	defer dstFile.Close()

	_, err = io.Copy(dstFile, srcFile)
	return err
}

func (m *OllamaManager) extractTarGz(archivePath string) error {
	file, err := os.Open(archivePath)
	if err != nil {
		return err
	}
	defer file.Close()

	gzr, err := gzip.NewReader(file)
	if err != nil {
		return err
	}
	defer gzr.Close()

	tmpDir, err := os.MkdirTemp("", "ollama-extract-*")
	if err != nil {
		return err
	}
	defer os.RemoveAll(tmpDir)

	cmd := exec.Command("tar", "-xf", "-", "-C", tmpDir)
	cmd.Stdin = gzr
	if err := cmd.Run(); err != nil {
		return err
	}

	ollamaBin := filepath.Join(tmpDir, "bin", "ollama")
	if _, err := os.Stat(ollamaBin); os.IsNotExist(err) {
		ollamaBin = filepath.Join(tmpDir, "ollama")
	}

	return m.copyBinary(ollamaBin)
}

func (m *OllamaManager) extractZip(archivePath string) error {
	r, err := zip.OpenReader(archivePath)
	if err != nil {
		return err
	}
	defer r.Close()

	for _, f := range r.File {
		if strings.HasSuffix(f.Name, "ollama.exe") {
			rc, err := f.Open()
			if err != nil {
				return err
			}
			defer rc.Close()

			outFile, err := os.Create(m.binPath)
			if err != nil {
				return err
			}
			defer outFile.Close()

			_, err = io.Copy(outFile, rc)
			return err
		}
	}

	return fmt.Errorf("ollama.exe not found in archive")
}

func (m *OllamaManager) GetOllamaCommand() string {
	if m.isInternal || m.IsInstalled() {
		return m.binPath
	}
	return "ollama"
}

func (m *OllamaManager) StartServer() (*exec.Cmd, error) {
	ollamaCmd := m.GetOllamaCommand()

	cmd := exec.Command(ollamaCmd, "serve")

	cmd.Env = append(os.Environ(),
		fmt.Sprintf("OLLAMA_MODELS=%s", m.modelsDir),
		"OLLAMA_HOST=127.0.0.1:11434",
	)

	cmd.Stdout = nil
	cmd.Stderr = nil

	if err := cmd.Start(); err != nil {
		return nil, err
	}

	for i := 0; i < 30; i++ {
		time.Sleep(500 * time.Millisecond)
		if IsOllamaRunning() {
			return cmd, nil
		}
	}

	cmd.Process.Kill()
	return nil, fmt.Errorf("ollama server failed to start")
}

func (m *OllamaManager) GetModelsDir() string {
	return m.modelsDir
}
