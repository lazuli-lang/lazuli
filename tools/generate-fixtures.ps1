param(
    [switch]$Check
)

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$ExamplesDir = Join-Path $RepoRoot "examples"

$PolicyAtomMap = [ordered]@{
    "role_admin"         = "@role.admin"
    "role_sales"         = "@role.sales"
    "sales_manager"      = "@role.sales_manager"
    "sales_ops"          = "@role.sales_ops"
    "integrations_admin" = "@role.integrations_admin"
    "finance_admin"      = "@role.finance_admin"
    "billing_admin"      = "@role.billing_admin"
    "support_agent"      = "@role.support_agent"
    "team_member"        = "@scope.team_member"
    "team_admin"         = "@role.team_admin"
    "account_owner"      = "@scope.account_owner"
    "same_org"           = "@scope.same_org"
    "same_team"          = "@scope.same_team"
    "self"               = "@scope.self"
    "owner"              = "@scope.owner"
    "authenticated"      = "@scope.authenticated"
    "org_member"         = "@scope.org_member"
    "org_admin"          = "@role.org_admin"
    "org_owner"          = "@role.org_owner"
    "public"             = "@scope.public"
    "system"             = "@actor.system"
    "none"               = "@scope.none"
}

function Replace-PolicyAtoms {
    param([string]$Line)

    $Updated = $Line
    foreach ($Entry in $PolicyAtomMap.GetEnumerator()) {
        $Pattern = "(?<![@A-Za-z0-9_\.])$([regex]::Escape($Entry.Key))(?![A-Za-z0-9_])"
        $Updated = [regex]::Replace($Updated, $Pattern, $Entry.Value)
    }
    return $Updated
}

function Convert-PolicyLines {
    param([string]$Text)

    $Lines = $Text -split "\r?\n", -1
    $Top = $null
    $Out = foreach ($Line in $Lines) {
        $Trimmed = $Line.TrimStart()
        $Leading = $Line.Length - $Trimmed.Length

        if ($Leading -eq 2 -and $Trimmed.Length -gt 0 -and -not $Trimmed.StartsWith("#")) {
            $Top = ($Trimmed -split "\s+")[0]
        }

        if (($Top -eq "policies" -and $Leading -ge 4) -or $Trimmed.StartsWith("policy ")) {
            Replace-PolicyAtoms $Line
        } else {
            $Line
        }
    }

    return ($Out -join "`n").TrimEnd("`n")
}

function Convert-FixtureText {
    param([string]$Text)

    $Updated = $Text

    $Updated = $Updated -replace "(?m)^(\s*)query\s+([A-Za-z_][A-Za-z0-9_]*)\s+raw\b", '$1query.sql $2'
    foreach ($Name in @("by_id", "by_email", "tag_by_id", "assignment_by_customer_tag", "by_customer")) {
        $Updated = $Updated -replace "(?m)^(\s*)query\s+$Name\b", "`$1query.lookup $Name"
    }
    $Updated = $Updated -replace "(?m)^(\s*)query\s+([A-Za-z_][A-Za-z0-9_]*)\b", '$1query.list $2'
    $Updated = $Updated -replace "(?m)^(\s*)query\.lookup\s+([A-Za-z_][A-Za-z0-9_]*)\r?\n\1  params\r?\n\1    ([A-Za-z_][A-Za-z0-9_]*): ([^\r\n]+)\r?\n\r?\n\1  key \3 = params\.\3", '$1query.lookup $2 by $3: $4'
    $Updated = $Updated -replace "(?m)^(\s*)events(\s+[A-Za-z_][A-Za-z0-9_]*_\*\s+on\s+[A-Za-z_][A-Za-z0-9_]*\s*)$", '$1event_group$2'

    $Updated = $Updated -replace ": Email\b", ": @semantic.Email"
    $Updated = $Updated -replace ": Money\b", ": @semantic.Money"
    $Updated = $Updated -replace ": File\b", ": @cap.File"
    $Updated = $Updated -replace ": Secret\b", ": @cap.Hashed(algorithm:argon2id)"
    $Updated = $Updated -replace "\b@cap\.Secret\b", "@cap.Hashed(algorithm:argon2id)"
    $Updated = $Updated -replace "Function\[Text, Secret\]", "Function[Text, @cap.Hashed(algorithm:argon2id)]"
    $Updated = $Updated -replace "Function\[Email, User\]", "Function[@semantic.Email, User]"

    $ExtensionRefs = [ordered]@{
        "ext.status_cell"              = "@client.status_cell"
        "ext.score_cell"               = "@client.score_cell"
        "ext.owner_cell"               = "@client.owner_cell"
        "ext.activity_timeline"        = "@client.activity_timeline"
        "ext.tag_editor"               = "@client.tag_editor"
        "ext.thread_view"              = "@client.thread_view"
        "ext.comment_thread"           = "@client.comment_thread"
        "ext.sub_issue_list"           = "@client.sub_issue_list"
        "ext.priority_cell"            = "@client.priority_cell"
        "ext.ltv_cell"                 = "@client.ltv_cell"
        "ext.risk_score"               = "@fn.risk_score"
        "ext.hash_customer_password"   = "@fn.hash_customer_password"
        "ext.verify_customer_password" = "@fn.verify_customer_password"
        "ext.enroll_customer_totp"     = "@fn.enroll_customer_totp"
        "ext.hash_password"            = "@fn.hash_password"
        "ext.verify_password"          = "@fn.verify_password"
        "ext.enroll_totp"              = "@fn.enroll_totp"
        "ext.resolve_or_invite_user"   = "@fn.resolve_or_invite_user"
        "ext.render_customer_archived" = "@fn.render_customer_archived"
        "ext.render_issue_assigned"    = "@fn.render_issue_assigned"
        "ext.next_invoice_number"      = "@fn.next_invoice_number"
        "ext.extract_mentions"         = "@fn.extract_mentions"
        "ext.google_oauth"             = "@adapter.google_oauth"
        "ext.verify_customer_totp"     = "@validator.verify_customer_totp"
        "ext.verify_totp"              = "@validator.verify_totp"
    }
    foreach ($Entry in $ExtensionRefs.GetEnumerator()) {
        $Updated = $Updated -replace "$([regex]::Escape($Entry.Key))\b", $Entry.Value
    }
    $Updated = $Updated -replace "(?m)^(\s*)server\s+([A-Za-z_][A-Za-z0-9_]*): Function", '$1fn $2: Function'
    $Updated = $Updated -replace "(?m)^(\s*)server\s+([A-Za-z_][A-Za-z0-9_]*): Hook", '$1hook $2: Hook'
    $Updated = $Updated -replace "(?m)^(\s*)server\s+([A-Za-z_][A-Za-z0-9_]*): Validator", '$1validator $2: Validator'
    $Updated = $Updated -replace "(?m)^(\s*)server\s+([A-Za-z_][A-Za-z0-9_]*): IntegrationAdapter", '$1adapter $2: IntegrationAdapter'
    $Updated = $Updated -replace "(?m)^(\s*)server\s+([A-Za-z_][A-Za-z0-9_]*): QueryModifier", '$1query_modifier $2: QueryModifier'

    $Updated = $Updated -replace "(?m)^(\s*)idempotency\s+(?!by\b)", '$1idempotency by '
    $Updated = $Updated -replace "\bidempotency by event\.id\b", "idempotency by envelope.id"
    $Updated = $Updated -replace "\bidempotency by event\.([A-Za-z_][A-Za-z0-9_]*)\b", 'idempotency by payload.$1'
    $Updated = $Updated -replace "\bevent\.(?!trace\b)([A-Za-z_][A-Za-z0-9_]*)", 'payload.$1'
    $Updated = $Updated -replace "\bevent\)", "payload)"
    $Updated = $Updated -replace "CustomerArchivedEvent", "CustomerArchivedPayload"
    $Updated = $Updated -replace "IssueAssignedEvent", "IssueAssignedPayload"
    $Updated = $Updated -creplace "(?m)^(\s*deny\s+[A-Z][A-Za-z0-9_]*\.[A-Za-z_][A-Za-z0-9_]*\s+when\s+)[a-z][A-Za-z0-9_]*\.", '$1self.'
    $Updated = $Updated -creplace "(\s+AND\s+)[a-z][A-Za-z0-9_]*\.", '$1self.'
    $Updated = $Updated -creplace "(\s+OR\s+)[a-z][A-Za-z0-9_]*\.", '$1self.'
    $Updated = $Updated -replace "(?m)^    target query\.by_id\(id: route\.id\)\r?\n", ""
    $Updated = $Updated -replace " when actor\.user\b", " when @actor.user"
    $Updated = $Updated -replace "(?m)^(\s*)event\s+([A-Za-z_][A-Za-z0-9_]*)\r?\n\1  observability_only", '$1event.trace $2'
    $Updated = $Updated -replace '(?m)^(\s*)validate\s+(")', '$1validates resource $2'
    $Updated = $Updated -replace '(?m)^(\s*)validates\s+field\s+resource\s+(")', '$1validates resource $2'
    $Updated = $Updated -replace '(?m)^(\s*)validates\s+(?!resource\b|field\b)([A-Za-z_][A-Za-z0-9_]*)\s+(")', '$1validates field $2 $3'
    $Updated = $Updated -replace '(?m)^(\s*)([A-Za-z_][A-Za-z0-9_]*):\s+(.+?)\s+previously\s+([A-Za-z_][A-Za-z0-9_]*)(\s*=\s*.*)?$', '$1$2 previously $4: $3$5'
    $Updated = $Updated -replace '(?m)^(\s*)allow\s+(@)', '$1permits $2'
    $Updated = $Updated -replace '(?m)^(\s*)deny\s+(@)', '$1forbids $2'
    $Updated = $Updated -replace "(?m)^(\s*)event customer_reassigned(\r?\n\1  )owner_id: ID", '$1event customer_reassigned$2to_owner_id: ID'
    $Updated = $Updated -replace "\bcustomer_tag_created\b", "tag_created"
    $Updated = $Updated -replace "\bcustomer_tag_assigned\b", "tag_assignment_created"
    $Updated = $Updated -replace "\bcustomer_tag_removed\b", "tag_assignment_removed"
    $Updated = $Updated -replace "(?m)^(\s{6})policy delete\b", '$1requires delete'
    $Updated = $Updated -replace "id customer_detail\b", "id @anchor.customer_detail"
    $Updated = $Updated -replace "extends @customer_detail\b", "extends @anchor.customer_detail"
    $Updated = $Updated -replace "(?m)^(\s*)view detail SidePanel id @anchor\.customer_detail(\r?\n)(?!\s*extensible_by\b)", '$1view detail SidePanel id @anchor.customer_detail$2$1  extensible_by customer_tags, customer_import$2'
    $Updated = $Updated -replace "(?m)^(\s*)view detail SidePanel id @anchor\.customer_detail(\r?\n)(?:\s*\r?\n)+\1  extensible_by", '$1view detail SidePanel id @anchor.customer_detail$2$1  extensible_by'
    $Updated = $Updated -replace "(?m)^(\s*extensible_by customer_tags, customer_import)(\r?\n)\1\b", '$1$2'
    $Updated = $Updated -replace "(?m)^(\s*extensible_by customer_tags, customer_import)(\r?\n)\s*\r?\n(\s*source )", '$1$2$3'
    $Updated = Convert-PolicyLines $Updated
    $Updated = $Updated -replace "(?m)^(\s*policy\s+)(create|update|delete|read|import|login|register|invite|global_read)(\s*)$", '$1@policy.$2$3'
    $Updated = $Updated -replace "(?m)^(\s*requires\s+)(create|update|delete|read|import|login|register|invite|global_read)(\s*)$", '$1@policy.$2$3'
    $Updated = $Updated -replace "(?m)(->\s+[A-Za-z_][A-Za-z0-9_]*\s+requires\s+)(create|update|delete|read|import|login|register|invite|global_read)(\s+emits\s+)", '$1@policy.$2$3'
    $Updated = $Updated -replace "(?m)^(\s*)policy\s+@actor\.system\s*$", '$1policy_for jobs, webhooks: @actor.system'
    $Updated = $Updated -replace "(?m)^(\s*policy_for jobs, webhooks: @actor\.system)(\r?\n)(?:[ \t]*\r?\n)*(\s*uses\b)", '$1$2$2$3'

    return $Updated.TrimEnd() + "`n"
}

$Changed = @()
$Files = Get-ChildItem $ExamplesDir -Recurse -Filter "*.lzi" | Where-Object { $_.Name -ne "crm.lzi" }

foreach ($File in $Files) {
    $Original = Get-Content -Raw $File.FullName
    $Newline = if ($Original.Contains("`r`n")) { "`r`n" } else { "`n" }
    $Generated = Convert-FixtureText $Original
    $Generated = [regex]::Replace($Generated, "\r?\n", $Newline)

    if ($Generated -ne $Original) {
        $Changed += $File.FullName

        if (-not $Check) {
            Set-Content -NoNewline -Path $File.FullName -Value $Generated
        }
    }
}

if ($Changed.Count -gt 0) {
    if ($Check) {
        Write-Error "Fixtures are not canonical. Run tools/generate-fixtures.ps1. Changed candidates: $($Changed -join ', ')"
    } else {
        Write-Host "Updated fixtures:"
        foreach ($Path in $Changed) {
            Write-Host "  $Path"
        }
    }
} else {
    Write-Host "Fixtures already canonical."
}
