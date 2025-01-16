#!/usr/bin/env bash

#SBATCH --output=.slurm/%j-fetch.out
#SBATCH  -N 1
#SBATCH  -n 16
#SBATCH  -p general
#SBATCH  --mem=16g
#SBATCH  -t 00-01:00:00

set -euo pipefail

ID=$1
OUTDIR="rawData"
THREADS=$2

mkdir -p $OUTDIR
vdb-config -Q yes
prefetch $ID -O ${OUTDIR}

cd ${OUTDIR}/${ID}/
fasterq-dump -e ${THREADS} ${ID}.sralite

rm ${ID}.sralite
