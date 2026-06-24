# Shoggoth Mesh Machine — Terraform Infrastructure
#
# Provisions cloud GPU instances for the Shoggoth fabric.
# Supports Brev.dev (experimental), AWS, and GCP.
#
# Usage:
#   terraform init
#   terraform plan -var="shoggoth_node_count=4"
#   terraform apply

terraform {
  required_version = ">= 1.8"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
    google = {
      source  = "hashicorp/google"
      version = "~> 6.0"
    }
  }
}

# ── Variables ──────────────────────────────────────────────────────────────────

variable "shoggoth_node_count" {
  description = "Number of cloud GPU nodes to provision"
  type        = number
  default     = 2
}

variable "instance_type" {
  description = "GPU instance type"
  type        = string
  default     = "g5.12xlarge"  # 4× A10G, 24 GB each
}

variable "shoggoth_version" {
  description = "Shoggoth node agent version tag"
  type        = string
  default     = "latest"
}

variable "orchestrator_addr" {
  description = "Shoggoth orchestrator address for node registration"
  type        = string
  default     = "xeon-brain-01.shoggoth.ts.net:9100"
}

variable "tailscale_auth_key" {
  description = "Tailscale pre-shared auth key"
  type        = string
  sensitive   = true
}

# ── AWS Provider ───────────────────────────────────────────────────────────────

provider "aws" {
  region = "us-east-1"
}

# Security group: allow QUIC and UDP heartbeat.
resource "aws_security_group" "shoggoth_node" {
  name        = "shoggoth-node-sg"
  description = "Shoggoth node agent traffic"

  ingress {
    from_port   = 9100
    to_port     = 9100
    protocol    = "udp"
    cidr_blocks = ["10.0.0.0/8", "100.64.0.0/10"]  # VPC + Tailscale
  }

  ingress {
    from_port   = 8888
    to_port     = 8888
    protocol    = "udp"
    cidr_blocks = ["10.0.0.0/8"]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

# GPU instances.
resource "aws_instance" "shoggoth_node" {
  count         = var.shoggoth_node_count
  ami           = "ami-0c1c3a1e4b8f5e8d0"  # Ubuntu 24.04 LTS with NVIDIA drivers
  instance_type = var.instance_type

  vpc_security_group_ids = [aws_security_group.shoggoth_node.id]

  root_block_device {
    volume_size = 200
    volume_type = "gp3"
  }

  user_data = templatefile("${path.module}/cloud-init.yml", {
    node_id           = "cloud-g5-${count.index}"
    orchestrator_addr = var.orchestrator_addr
    shoggoth_version  = var.shoggoth_version
    tailscale_key     = var.tailscale_auth_key
  })

  tags = {
    Name    = "shoggoth-cloud-node-${count.index}"
    Project = "shoggoth"
    Role    = "compute-node"
  }
}

# ── Outputs ────────────────────────────────────────────────────────────────────

output "node_public_ips" {
  description = "Public IPs of provisioned cloud nodes"
  value       = aws_instance.shoggoth_node[*].public_ip
}

output "node_private_ips" {
  description = "Private IPs for QUIC connections"
  value       = aws_instance.shoggoth_node[*].private_ip
}
