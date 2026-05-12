package migrations

import (
	"errors"
	"reflect"
	"testing"
	"testing/fstest"
)

func TestNewAtlasMigrationFileParsesMetadataAndChecksum(t *testing.T) {
	data := []byte("CREATE TABLE users (id bigint);\n")

	file, err := NewAtlasMigrationFile("db/migrations/000002_add_users.up.sql", data)
	if err != nil {
		t.Fatalf("NewAtlasMigrationFile returned %v", err)
	}

	if file.Path != "db/migrations/000002_add_users.up.sql" {
		t.Fatalf("Path = %q", file.Path)
	}
	if file.Version != "000002" {
		t.Fatalf("Version = %q, want 000002", file.Version)
	}
	if file.Name != "add_users" {
		t.Fatalf("Name = %q, want add_users", file.Name)
	}
	if file.Direction != AtlasMigrationDirectionUp {
		t.Fatalf("Direction = %q, want %q", file.Direction, AtlasMigrationDirectionUp)
	}
	if file.Size != int64(len(data)) {
		t.Fatalf("Size = %d, want %d", file.Size, len(data))
	}
	if file.Checksum != AtlasMigrationChecksum(data) {
		t.Fatalf("Checksum = %q, want %q", file.Checksum, AtlasMigrationChecksum(data))
	}
}

func TestParseAtlasMigrationFilePathSupportsAtlasStyleFiles(t *testing.T) {
	file, err := ParseAtlasMigrationFilePath("migrations/20240501090000_init.sql")
	if err != nil {
		t.Fatalf("ParseAtlasMigrationFilePath returned %v", err)
	}

	if file.Version != "20240501090000" || file.Name != "init" || file.Direction != AtlasMigrationDirectionNone {
		t.Fatalf("metadata = %#v", file)
	}
}

func TestLoadAtlasMigrationManifestDiscoversSQLInDeterministicOrder(t *testing.T) {
	source := fstest.MapFS{
		"db/migrations/000002_add_email.up.sql":       &fstest.MapFile{Data: []byte("ALTER TABLE users ADD COLUMN email text;\n")},
		"db/migrations/000001_create_users.down.sql":  &fstest.MapFile{Data: []byte("DROP TABLE users;\n")},
		"db/migrations/000001_create_users.up.sql":    &fstest.MapFile{Data: []byte("CREATE TABLE users (id bigint);\n")},
		"db/migrations/nested/000003_add_posts.sql":   &fstest.MapFile{Data: []byte("CREATE TABLE posts (id bigint);\n")},
		"db/migrations/nested/README.md":              &fstest.MapFile{Data: []byte("notes\n")},
		"db/migrations/nested/atlas.sum":              &fstest.MapFile{Data: []byte("ignored\n")},
		"db/migrations/nested/000004_add_comments.md": &fstest.MapFile{Data: []byte("ignored\n")},
	}

	manifest, err := LoadAtlasMigrationManifest(source, "db/migrations")
	if err != nil {
		t.Fatalf("LoadAtlasMigrationManifest returned %v", err)
	}

	got := atlasMigrationTestPaths(manifest.Files)
	want := []string{
		"db/migrations/000001_create_users.up.sql",
		"db/migrations/000001_create_users.down.sql",
		"db/migrations/000002_add_email.up.sql",
		"db/migrations/nested/000003_add_posts.sql",
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("manifest files = %v, want %v", got, want)
	}
	if manifest.Files[0].Checksum != AtlasMigrationChecksum([]byte("CREATE TABLE users (id bigint);\n")) {
		t.Fatalf("first checksum = %q", manifest.Files[0].Checksum)
	}
}

func TestValidateAtlasMigrationOrderRejectsOutOfOrderFiles(t *testing.T) {
	second := atlasMigrationTestFile(t, "000002_second.sql", "second\n")
	first := atlasMigrationTestFile(t, "000001_first.sql", "first\n")

	err := ValidateAtlasMigrationOrder([]AtlasMigrationFile{second, first})
	if !errors.Is(err, ErrAtlasMigrationOrderInvalid) {
		t.Fatalf("ValidateAtlasMigrationOrder error = %v, want ErrAtlasMigrationOrderInvalid", err)
	}
}

func TestNewAtlasMigrationManifestRejectsDuplicateVersion(t *testing.T) {
	first := atlasMigrationTestFile(t, "000001_create_users.up.sql", "create users\n")
	duplicate := atlasMigrationTestFile(t, "1_create_accounts.up.sql", "create accounts\n")

	_, err := NewAtlasMigrationManifest([]AtlasMigrationFile{first, duplicate})
	if !errors.Is(err, ErrDuplicateAtlasMigrationVersion) {
		t.Fatalf("NewAtlasMigrationManifest error = %v, want ErrDuplicateAtlasMigrationVersion", err)
	}
}

func TestRenderAtlasMigrationManifestIsDeterministic(t *testing.T) {
	first := atlasMigrationTestFile(t, "000001_create_users.up.sql", "create users\n")
	second := atlasMigrationTestFile(t, "000002_add_email.up.sql", "add email\n")

	left, err := RenderAtlasMigrationManifest(AtlasMigrationManifest{Files: []AtlasMigrationFile{second, first}})
	if err != nil {
		t.Fatalf("RenderAtlasMigrationManifest(left) returned %v", err)
	}
	right, err := RenderAtlasMigrationManifest(AtlasMigrationManifest{Files: []AtlasMigrationFile{first, second}})
	if err != nil {
		t.Fatalf("RenderAtlasMigrationManifest(right) returned %v", err)
	}
	if left != right {
		t.Fatalf("rendered manifests differ:\nleft:\n%s\nright:\n%s", left, right)
	}

	body := first.Path + " " + first.Checksum + "\n" + second.Path + " " + second.Checksum + "\n"
	want := AtlasMigrationChecksum([]byte(body)) + "\n" + body
	if left != want {
		t.Fatalf("rendered manifest = %q, want %q", left, want)
	}
}

func TestParseAtlasMigrationFilePathRejectsMalformedFiles(t *testing.T) {
	tests := []string{
		"README.md",
		"000001.sql",
		"abc_init.sql",
		"000001_create users.sql",
		"../000001_init.sql",
	}

	for _, file := range tests {
		t.Run(file, func(t *testing.T) {
			_, err := ParseAtlasMigrationFilePath(file)
			if !errors.Is(err, ErrInvalidAtlasMigrationFile) {
				t.Fatalf("ParseAtlasMigrationFilePath error = %v, want ErrInvalidAtlasMigrationFile", err)
			}
		})
	}
}

func atlasMigrationTestFile(t *testing.T, name, data string) AtlasMigrationFile {
	t.Helper()
	file, err := NewAtlasMigrationFile(name, []byte(data))
	if err != nil {
		t.Fatalf("NewAtlasMigrationFile(%q) returned %v", name, err)
	}
	return file
}

func atlasMigrationTestPaths(files []AtlasMigrationFile) []string {
	paths := make([]string, len(files))
	for i, file := range files {
		paths[i] = file.Path
	}
	return paths
}
